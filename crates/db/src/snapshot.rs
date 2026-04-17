//! A point-in-time view of everything the solver needs for one practice.

use crate::availability::{types::AvailabilityStatus, Availability};
use crate::boat::Boat;
use crate::lineup::{Lineup, RecentPlacement};
use crate::pair_affinity::PairAffinity;
use crate::practice::Practice;
use crate::rower::{types::RowerId, Rower};
use crate::seat_affinity::SeatAffinity;
use crate::team::{Team, TeamMembership};
use chrono::NaiveDate;
use diesel::SqliteConnection;
use std::collections::HashMap;

/// How many recent practices feed into [`DbSnapshot::recent_placements`].
/// Used by the S7 novelty soft constraint to detect "the same person in
/// the same seat of the same boat every Tuesday" drift.
pub const RECENT_LINEUP_WINDOW: i64 = 4;

#[derive(Debug, Clone)]
pub struct DbSnapshot {
    pub date: NaiveDate,
    pub rowers: Vec<Rower>,
    /// When true, rowers who haven't responded are treated as available.
    /// Loaded from the team's `assume_available` flag.
    pub assume_available: bool,
    /// Availability status for each rower that explicitly responded. Rowers
    /// not in this map are treated as "unset" — interpreted as available or
    /// unavailable depending on [`assume_available`].
    pub availability: HashMap<RowerId, AvailabilityStatus>,
    /// All in-service boats (sweep + scull). The solver uses `sweep_bias`
    /// on each rower to determine eligibility for sweep vs scull boats.
    pub boats: Vec<Boat>,
    /// Derived from `lineup_seat` history.
    pub last_coxed: HashMap<RowerId, NaiveDate>,
    /// Most recent committed practice where each rower was available
    /// but not placed in any lineup. Drives S20 bench cooldown.
    pub last_benched: HashMap<RowerId, NaiveDate>,
    /// Per-rower seat preferences (boat-agnostic position). Drives S3.
    pub seat_affinities: Vec<SeatAffinity>,
    /// Rower-pair affinities (two rowers should form a rowing pair — a
    /// fixed 2-seat partition of a boat). Drives S2.
    pub pair_affinities: Vec<PairAffinity>,
    /// Flattened placements from the `RECENT_LINEUP_WINDOW` most recent
    /// committed practices. Each entry is a `(practice_date, boat_id,
    /// seat_position, rower_id, is_cox)` tuple. Drives S7 novelty —
    /// the solver penalises placing the same rower into a seat they
    /// recently occupied.
    pub recent_placements: Vec<RecentPlacement>,
}

impl DbSnapshot {
    /// Build a snapshot for a specific practice.
    ///
    /// Rowers are filtered to those on the team (via `team_membership`).
    /// Availability is scoped to the practice. Boats are the full shared
    /// fleet (not team-filtered). Affinities are loaded for all rowers
    /// on the team.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn for_practice(
        conn: &mut SqliteConnection,
        practice: &Practice,
    ) -> Result<Self, diesel::result::Error> {
        let team_id = practice.team_id;

        // Rowers on this team (active only).
        let team_rower_ids = TeamMembership::rower_ids_for_team(conn, team_id)?;
        let all_active = Rower::list_active(conn)?;
        let rowers: Vec<Rower> = all_active
            .into_iter()
            .filter(|r| team_rower_ids.contains(&r.id))
            .collect();

        let assume_available = Team::get(conn, team_id)?
            .map(|t| t.assume_available.as_bool())
            .unwrap_or(false);

        Ok(Self {
            date: practice.date,
            assume_available,
            availability: Availability::map_for_practice(conn, practice.id)?,
            boats: Boat::list_in_service(conn)?,
            last_coxed: Rower::last_coxed_dates(conn)?,
            last_benched: Rower::last_benched_dates(conn)?,
            seat_affinities: SeatAffinity::list_all(conn)?,
            pair_affinities: PairAffinity::list_all(conn)?,
            recent_placements: Lineup::recent_placements(conn, RECENT_LINEUP_WINDOW)?,
            rowers,
        })
    }

    /// Rowers who responded "Yes" to this practice. The solver uses
    /// `sweep_bias` to determine which boats each rower is eligible for.
    pub fn available_rowers(&self) -> impl Iterator<Item = &Rower> {
        self.rowers.iter().filter(|r| {
            self.availability
                .get(&r.id)
                .map(|s| s.is_available_for_sweep())
                .unwrap_or(self.assume_available)
        })
    }
}

impl std::fmt::Display for DbSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Lineup snapshot for {} ===", self.date)?;

        writeln!(f, "\nBoats ({})", self.boats.len())?;
        for b in &self.boats {
            writeln!(
                f,
                "  #{:<3} {:<24} {:<7} seats={} cox={}",
                b.id,
                b.name,
                b.weight_class.to_string(),
                b.seat_count,
                b.has_cox.as_bool()
            )?;
        }

        let available: Vec<_> = self.available_rowers().collect();
        writeln!(
            f,
            "\nRowers ({} total, {} available for sweep)",
            self.rowers.len(),
            available.len()
        )?;
        for r in &self.rowers {
            let status = self
                .availability
                .get(&r.id)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unset".to_string());
            let flags = format!(
                "{}{}b={}",
                if r.can_cox.as_bool() { "C" } else { "" },
                if r.is_designated_cox.as_bool() {
                    "*"
                } else {
                    ""
                },
                r.sweep_bias.as_int(),
            );
            let last_cox = self
                .last_coxed
                .get(&r.id)
                .map(|d| d.to_string())
                .unwrap_or_else(|| "-".to_string());
            writeln!(
                f,
                "  #{:<4} {:<20} {} {} {} side={:<12} ({}) {:<4} avail={:<15} last_cox={}",
                r.id,
                r.name,
                r.weight_class,
                r.skill,
                r.strength,
                r.side.to_string(),
                r.side_strength,
                flags,
                status,
                last_cox,
            )?;
        }
        Ok(())
    }
}
