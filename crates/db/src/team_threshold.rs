//! Per-team threshold config for auto-bucketing raw rower metrics
//! into categorical enums (weight class, height, strength).
//!
//! Each row defines 3 boundary values that split a metric range into
//! 4 buckets. The `metric` column is one of "weight", "height", or
//! "strength".

use crate::schema::team_threshold;
use crate::team::TeamId;
use diesel::prelude::*;
use diesel::SqliteConnection;

/// Threshold metric identifiers.
pub const METRIC_WEIGHT: &str = "weight";
pub const METRIC_HEIGHT: &str = "height";
pub const METRIC_STRENGTH: &str = "strength";

#[derive(Debug, Clone, diesel::Queryable, diesel::Selectable, diesel::Insertable)]
#[diesel(table_name = team_threshold)]
pub struct TeamThreshold {
    pub team_id: TeamId,
    /// One of "weight", "height", "strength".
    pub metric: String,
    /// Boundary between bucket 1 (lowest) and bucket 2.
    pub low_mid: f64,
    /// Boundary between bucket 2 and bucket 3.
    pub mid_high: f64,
    /// Boundary between bucket 3 and bucket 4 (highest).
    pub high_very: f64,
}

impl TeamThreshold {
    /// Load all thresholds for a team (0–3 rows).
    pub fn for_team(
        conn: &mut SqliteConnection,
        team_id: TeamId,
    ) -> Result<Vec<TeamThreshold>, diesel::result::Error> {
        team_threshold::table
            .filter(team_threshold::team_id.eq(team_id))
            .select(TeamThreshold::as_select())
            .get_results(conn)
    }

    /// Load a specific metric's thresholds for a team.
    pub fn get(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        metric: &str,
    ) -> Result<Option<TeamThreshold>, diesel::result::Error> {
        team_threshold::table
            .find((team_id, metric))
            .select(TeamThreshold::as_select())
            .first(conn)
            .optional()
    }

    /// Upsert a threshold row. Creates or replaces.
    pub fn upsert(
        conn: &mut SqliteConnection,
        row: &TeamThreshold,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(team_threshold::table)
            .values(row)
            .on_conflict((team_threshold::team_id, team_threshold::metric))
            .do_update()
            .set((
                team_threshold::low_mid.eq(row.low_mid),
                team_threshold::mid_high.eq(row.mid_high),
                team_threshold::high_very.eq(row.high_very),
            ))
            .execute(conn)?;
        Ok(())
    }

    /// Delete a threshold row.
    pub fn delete(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        metric: &str,
    ) -> Result<(), diesel::result::Error> {
        diesel::delete(team_threshold::table.find((team_id, metric))).execute(conn)?;
        Ok(())
    }

    /// Given a raw value and 3 boundaries, return which bucket (0–3) it
    /// falls into. Bucket 0 = below low_mid, bucket 3 = above high_very.
    /// For "ascending" metrics (weight, height) — higher value = higher bucket.
    pub fn bucket_ascending(value: f64, low_mid: f64, mid_high: f64, high_very: f64) -> u8 {
        if value >= high_very {
            3
        } else if value >= mid_high {
            2
        } else if value >= low_mid {
            1
        } else {
            0
        }
    }

    /// For "descending" metrics (erg split — lower time = stronger).
    /// Bucket 0 = slowest (above high_very), bucket 3 = fastest (below low_mid).
    pub fn bucket_descending(value: f64, low_mid: f64, mid_high: f64, high_very: f64) -> u8 {
        if value <= high_very {
            3
        } else if value <= mid_high {
            2
        } else if value <= low_mid {
            1
        } else {
            0
        }
    }
}

use crate::erg_test::ErgTest;
use crate::rower::types::{Height, RowerWeightClass, Strength};
use crate::rower::Rower;

/// Derive weight class from raw kg using ascending thresholds (heavier = higher bucket).
pub fn derive_weight_class(kg: f64, t: &TeamThreshold) -> RowerWeightClass {
    match TeamThreshold::bucket_ascending(kg, t.low_mid, t.mid_high, t.high_very) {
        0 => RowerWeightClass::Light,
        1 => RowerWeightClass::Medium,
        2 => RowerWeightClass::Heavy,
        _ => RowerWeightClass::VeryHeavy,
    }
}

/// Derive height from raw metres using ascending thresholds.
pub fn derive_height(m: f64, t: &TeamThreshold) -> Height {
    match TeamThreshold::bucket_ascending(m, t.low_mid, t.mid_high, t.high_very) {
        0 => Height::Short,
        1 => Height::Medium,
        2 => Height::Tall,
        _ => Height::VeryTall,
    }
}

/// Derive strength from erg split (cs/500m) using descending thresholds
/// (faster split = stronger).
pub fn derive_strength(split_cs: f64, t: &TeamThreshold) -> Strength {
    match TeamThreshold::bucket_descending(split_cs, t.low_mid, t.mid_high, t.high_very) {
        0 => Strength::Weak,
        1 => Strength::Intermediate,
        2 => Strength::Strong,
        _ => Strength::VeryStrong,
    }
}

/// Batch-recalculate categorical buckets for all rowers on a team that
/// have raw values. Returns the number of rowers updated.
///
/// **Known limitation:** categorical fields (weight_class, height,
/// strength) are global on the rower, but thresholds are per-team. If
/// a rower belongs to multiple teams with different thresholds, the
/// last team to save thresholds determines that rower's categories.
pub fn batch_derive(
    conn: &mut SqliteConnection,
    team_id: TeamId,
    thresholds: &[TeamThreshold],
    erg_distance: Option<i32>,
) -> Result<usize, diesel::result::Error> {
    use crate::schema::{rower, team_membership};
    use std::collections::HashMap;

    let weight_t = thresholds.iter().find(|t| t.metric == METRIC_WEIGHT);
    let height_t = thresholds.iter().find(|t| t.metric == METRIC_HEIGHT);
    let strength_t = thresholds.iter().find(|t| t.metric == METRIC_STRENGTH);

    // Load team rowers.
    let rower_ids: Vec<crate::rower::types::RowerId> = team_membership::table
        .filter(team_membership::team_id.eq(team_id))
        .select(team_membership::rower_id)
        .get_results(conn)?;

    let rowers: Vec<Rower> = rower::table
        .filter(rower::id.eq_any(&rower_ids))
        .select(Rower::as_select())
        .get_results(conn)?;

    // Load latest erg test per rower at the threshold distance.
    let erg_by_rower: HashMap<crate::rower::types::RowerId, ErgTest> =
        if let Some(dist) = erg_distance {
            if strength_t.is_some() {
                let all_tests: Vec<ErgTest> = crate::schema::erg_test::table
                    .filter(crate::schema::erg_test::rower_id.eq_any(&rower_ids))
                    .filter(crate::schema::erg_test::distance_m.eq(dist))
                    .select(ErgTest::as_select())
                    .order(crate::schema::erg_test::created_at.desc())
                    .get_results(conn)?;
                let mut map = HashMap::new();
                for t in all_tests {
                    map.entry(t.rower_id).or_insert(t);
                }
                map
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

    let mut updated = 0usize;
    let now = chrono::Utc::now().naive_utc();

    for r in &rowers {
        let mut changed = false;
        let mut new_wc = r.weight_class;
        let mut new_h = r.height;
        let mut new_s = r.strength;

        if let (Some(kg), Some(t)) = (r.weight_kg, weight_t) {
            let derived = derive_weight_class(kg.as_f64(), t);
            if derived != r.weight_class {
                new_wc = derived;
                changed = true;
            }
        }
        if let (Some(m), Some(t)) = (r.height_m, height_t) {
            let derived = derive_height(m.as_f64(), t);
            if derived != r.height {
                new_h = derived;
                changed = true;
            }
        }
        if let Some(t) = strength_t {
            if let Some(erg) = erg_by_rower.get(&r.id) {
                let split_cs = (erg.time_cs as f64) / (erg.distance_m as f64 / 500.0);
                let derived = derive_strength(split_cs, t);
                if derived != r.strength {
                    new_s = derived;
                    changed = true;
                }
            }
        }

        if changed {
            diesel::update(rower::table.find(r.id))
                .set((
                    rower::weight_class.eq(new_wc),
                    rower::height.eq(new_h),
                    rower::strength.eq(new_s),
                    rower::updated_at.eq(now),
                ))
                .execute(conn)?;
            updated += 1;
        }
    }

    Ok(updated)
}
