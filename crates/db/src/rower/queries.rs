use super::types::{RowerId, Side};
use super::{NewRower, Rower};
use crate::schema::{availability, lineup, lineup_seat, practice, rower};
use crate::types::IntBool;
use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::SqliteConnection;
use std::collections::HashMap;

impl Rower {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn insert(
        conn: &mut SqliteConnection,
        new: NewRower,
    ) -> Result<Rower, diesel::result::Error> {
        diesel::insert_into(rower::table)
            .values(new)
            .returning(Rower::as_returning())
            .get_result(conn)
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_active(conn: &mut SqliteConnection) -> Result<Vec<Rower>, diesel::result::Error> {
        rower::table
            .filter(rower::active.eq(1))
            .select(Rower::as_select())
            .order(rower::name.asc())
            .get_results(conn)
    }

    /// Look up a rower by id. Returns `Ok(None)` for an unknown id —
    /// callers convert that into whatever they need (typically a 404).
    /// Other database errors propagate as `Err`.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn get(
        conn: &mut SqliteConnection,
        id: RowerId,
    ) -> Result<Option<Rower>, diesel::result::Error> {
        rower::table
            .find(id)
            .select(Rower::as_select())
            .first(conn)
            .optional()
    }

    /// Persist all fields of `rower` to the matching row, bumping
    /// `updated_at` to now. Used by the admin UI's inline-edit flow:
    /// load via [`Rower::get`], mutate the editable fields, then call
    /// `save`. Returns the freshly-loaded row so callers can render
    /// the canonical state without an extra round trip.
    ///
    /// Distinct from [`Rower::promote_from_sheet`], which is the
    /// promote-only sync path. `save` is the unrestricted path:
    /// caller-supplied values land verbatim. Validate at the call site.
    #[tracing::instrument(level = "debug", skip(conn, rower), err)]
    pub fn save(
        conn: &mut SqliteConnection,
        rower: &Rower,
    ) -> Result<Rower, diesel::result::Error> {
        let mut next = rower.clone();
        next.updated_at = chrono::Utc::now().naive_utc();
        diesel::update(rower::table.filter(rower::id.eq(rower.id)))
            .set(&next)
            .returning(Rower::as_returning())
            .get_result(conn)
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn count(conn: &mut SqliteConnection) -> Result<i64, diesel::result::Error> {
        rower::table.count().get_result(conn)
    }

    /// When each rower last coxed, derived from the `lineup_seat` history.
    /// Rowers who have never coxed are absent from the returned map.
    ///
    /// Replaces a denormalised `rower.last_coxed_on` column — the lineup
    /// history is the source of truth.
    ///
    /// We load every cox appearance and reduce in Rust rather than asking
    /// diesel to emit a `GROUP BY rower_id, MAX(date)` query; the latter is
    /// finicky to type across a three-table join in diesel 2.x and the
    /// dataset is tiny (bounded by cox appearances ever).
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn last_coxed_dates(
        conn: &mut SqliteConnection,
    ) -> Result<HashMap<RowerId, NaiveDate>, diesel::result::Error> {
        let rows: Vec<(RowerId, NaiveDate)> = lineup_seat::table
            .inner_join(lineup::table.on(lineup::id.eq(lineup_seat::lineup_id)))
            .inner_join(practice::table.on(practice::id.eq(lineup::practice_id)))
            .filter(lineup_seat::is_cox.eq(1))
            .select((lineup_seat::rower_id, practice::date))
            .load(conn)?;

        let mut map: HashMap<RowerId, NaiveDate> = HashMap::new();
        for (rid, date) in rows {
            map.entry(rid)
                .and_modify(|existing| {
                    if date > *existing {
                        *existing = date;
                    }
                })
                .or_insert(date);
        }
        Ok(map)
    }

    /// For each rower, find the most recent committed practice date
    /// where they were available (status = Yes) but not placed in any
    /// lineup seat. "Benched" = available AND not in lineup_seat.
    ///
    /// Strategy: load all (rower_id, date) pairs where the rower had
    /// availability = Yes for a practice that has at least one committed
    /// lineup, then subtract rowers who appear in a lineup_seat for
    /// that practice. Reduce per-rower to the most recent such date.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn last_benched_dates(
        conn: &mut SqliteConnection,
    ) -> Result<HashMap<RowerId, NaiveDate>, diesel::result::Error> {
        use crate::availability::types::AvailabilityStatus;
        use std::collections::HashSet;

        // Step 1: all (rower, date) pairs where the rower was available
        // for a committed practice (practice has at least one lineup).
        let committed_practice_ids: Vec<i32> = practice::table
            .filter(practice::id.eq_any(lineup::table.select(lineup::practice_id)))
            .select(practice::id)
            .get_results(conn)?;

        if committed_practice_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Load the practice dates for committed practices.
        let committed_practices: Vec<(i32, NaiveDate)> = practice::table
            .filter(practice::id.eq_any(&committed_practice_ids))
            .select((practice::id, practice::date))
            .get_results(conn)?;
        let practice_id_to_date: HashMap<i32, NaiveDate> = committed_practices
            .iter()
            .map(|(id, d)| (*id, *d))
            .collect();

        // All available rowers per committed practice.
        let avail_rows: Vec<(RowerId, i32, AvailabilityStatus)> = availability::table
            .filter(availability::practice_id.eq_any(&committed_practice_ids))
            .select((availability::rower_id, availability::practice_id, availability::status))
            .get_results(conn)?;

        let mut available_by_date: HashMap<NaiveDate, HashSet<RowerId>> = HashMap::new();
        for (rid, pid, status) in &avail_rows {
            if status.is_available_for_sweep() {
                if let Some(&date) = practice_id_to_date.get(pid) {
                    available_by_date.entry(date).or_default().insert(*rid);
                }
            }
        }

        // Step 2: all placed rowers per committed practice.
        let placed_rows: Vec<(RowerId, i32)> = lineup_seat::table
            .inner_join(lineup::table.on(lineup::id.eq(lineup_seat::lineup_id)))
            .filter(lineup::practice_id.eq_any(&committed_practice_ids))
            .select((lineup_seat::rower_id, lineup::practice_id))
            .get_results(conn)?;

        let mut placed_by_date: HashMap<NaiveDate, HashSet<RowerId>> = HashMap::new();
        for (rid, pid) in &placed_rows {
            if let Some(&date) = practice_id_to_date.get(pid) {
                placed_by_date.entry(date).or_default().insert(*rid);
            }
        }

        // Step 3: benched = available - placed, per date. Keep most recent.
        let mut map: HashMap<RowerId, NaiveDate> = HashMap::new();
        for (date, available) in &available_by_date {
            let placed = placed_by_date.get(date);
            for rid in available {
                let was_placed = placed.map(|s| s.contains(rid)).unwrap_or(false);
                if !was_placed {
                    map.entry(*rid)
                        .and_modify(|existing| {
                            if *date > *existing {
                                *existing = *date;
                            }
                        })
                        .or_insert(*date);
                }
            }
        }
        Ok(map)
    }

    /// Promote-only update of mutable rower attributes from sheet data.
    ///
    /// Rule: the sync path NEVER demotes a coach-set value. Specific
    /// sides stay specific, true flags stay true. The sync only
    /// promotes:
    /// - `side`: `Either` → `Port` or `Starboard`
    /// - `can_cox`: `false` → `true`
    /// - `is_designated_cox`: `false` → `true`
    /// - `sweep_bias`: sculling rows always set -2 (team column is
    ///   authoritative for "I'm a sculler"); sweep rows don't override
    ///   existing coach-set values.
    ///
    /// Also updates `name` unconditionally (the sheet is the authoritative
    /// display name) and `updated_at` to now.
    ///
    /// Returns the updated Rower. If no fields changed, the existing
    /// row is returned unchanged and no UPDATE is issued.
    #[tracing::instrument(level = "debug", skip(conn, current), err)]
    pub fn promote_from_sheet(
        conn: &mut SqliteConnection,
        current: &Rower,
        new_name: &str,
        new_side: Side,
        is_sculling: bool,
        new_can_cox: bool,
        new_is_designated_cox: bool,
    ) -> Result<Rower, diesel::result::Error> {
        use super::types::SweepBias;
        let mut dirty = false;
        let mut next = current.clone();

        if next.name != new_name {
            next.name = new_name.to_string();
            dirty = true;
        }
        // Side: promote Either → specific; never demote.
        if next.side == Side::Either && new_side != Side::Either {
            next.side = new_side;
            dirty = true;
        }
        // Sculling rows always force -2 (team column is authoritative).
        if is_sculling && next.sweep_bias != SweepBias::SCULL_HARD {
            next.sweep_bias = SweepBias::SCULL_HARD;
            dirty = true;
        }
        // Boolean flags: promote false → true; never demote.
        if !next.can_cox.as_bool() && new_can_cox {
            next.can_cox = IntBool::TRUE;
            dirty = true;
        }
        if !next.is_designated_cox.as_bool() && new_is_designated_cox {
            next.is_designated_cox = IntBool::TRUE;
            dirty = true;
        }

        if !dirty {
            return Ok(current.clone());
        }

        next.updated_at = chrono::Utc::now().naive_utc();
        diesel::update(rower::table.filter(rower::id.eq(current.id)))
            .set(&next)
            .returning(Rower::as_returning())
            .get_result(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rower::types::{Height, RowerWeightClass, SideStrength, Skill, Strength, SweepBias};
    use crate::rower::NewRower;
    use crate::test_support::in_memory_conn;
    use crate::types::IntBool;

    fn seed_rower(conn: &mut SqliteConnection, side: Side) -> Rower {
        let now = chrono::Utc::now().naive_utc();
        Rower::insert(
            conn,
            NewRower {
                name: "Seed Rower".into(),
                weight_class: RowerWeightClass::Medium,
                skill: Skill::Intermediate,
                strength: Strength::Intermediate,
                height: Height::Medium,
                side,
                side_strength: SideStrength::default(),
                sweep_bias: SweepBias::default(),
                can_cox: IntBool::TRUE,
                is_designated_cox: IntBool::FALSE,
                active: IntBool::TRUE,
                created_at: now,
                updated_at: now,
            },
        )
        .expect("seed rower insert")
    }

    #[test]
    fn promote_from_sheet_never_demotes_specific_side_to_either() {
        let mut conn = in_memory_conn();
        let seeded = seed_rower(&mut conn, Side::Starboard);

        let updated = Rower::promote_from_sheet(
            &mut conn, &seeded, "Alice Smith", Side::Either, false, true, false,
        ).unwrap();

        assert_eq!(updated.side, Side::Starboard);
        assert_eq!(updated.name, "Alice Smith");
    }

    #[test]
    fn promote_from_sheet_does_promote_either_to_specific() {
        let mut conn = in_memory_conn();
        let seeded = seed_rower(&mut conn, Side::Either);

        let updated = Rower::promote_from_sheet(
            &mut conn, &seeded, "Alice Smith", Side::Port, false, true, false,
        ).unwrap();

        assert_eq!(updated.side, Side::Port);
    }

    #[test]
    fn promote_from_sheet_sculling_row_forces_scull_hard() {
        let mut conn = in_memory_conn();
        let seeded = seed_rower(&mut conn, Side::Port);
        assert_eq!(seeded.sweep_bias, SweepBias::default());

        let updated = Rower::promote_from_sheet(
            &mut conn, &seeded, &seeded.name, Side::Port, true, true, false,
        ).unwrap();

        assert_eq!(updated.sweep_bias, SweepBias::SCULL_HARD);
    }

    #[test]
    fn promote_from_sheet_sweep_row_does_not_override_bias() {
        let mut conn = in_memory_conn();
        let seeded = seed_rower(&mut conn, Side::Port);
        // Coach set bias to 1 (prefers sweep).
        let mut adjusted = seeded.clone();
        adjusted.sweep_bias = SweepBias::new(1);
        let adjusted = Rower::save(&mut conn, &adjusted).unwrap();

        let updated = Rower::promote_from_sheet(
            &mut conn, &adjusted, &adjusted.name, Side::Port, false, true, false,
        ).unwrap();

        assert_eq!(updated.sweep_bias, SweepBias::new(1), "sweep row should not override coach-set bias");
    }

    #[test]
    fn promote_from_sheet_is_noop_when_nothing_changed() {
        let mut conn = in_memory_conn();
        let seeded = seed_rower(&mut conn, Side::Port);
        let seeded_updated_at = seeded.updated_at;

        let updated = Rower::promote_from_sheet(
            &mut conn, &seeded, &seeded.name, Side::Port, false, true, false,
        ).unwrap();

        assert_eq!(updated.updated_at, seeded_updated_at, "no-op promote should not touch updated_at");
    }

    #[test]
    fn promote_from_sheet_updates_name_unconditionally() {
        let mut conn = in_memory_conn();
        let seeded = seed_rower(&mut conn, Side::Port);

        let updated = Rower::promote_from_sheet(
            &mut conn, &seeded, "Alice Married-Name", Side::Port, false, true, false,
        ).unwrap();

        assert_eq!(updated.name, "Alice Married-Name");
        assert_ne!(updated.updated_at, seeded.updated_at, "a name change should bump updated_at");
    }
}
