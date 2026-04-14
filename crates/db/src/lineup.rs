//! Committed lineups: the historical record of (practice, boat, seat,
//! rower) assignments that the solver has persisted as "this is what
//! actually went out today".
//!
//! The write path is `commit_for_boat` — replace any existing committed
//! lineup for a given (practice, boat) pair and insert the provided seat
//! assignments. Replace-on-conflict keeps the table idempotent so a
//! re-solve + re-commit cycle doesn't leave stale rows.
//!
//! The read path is used by history-driven soft constraints:
//! - `Rower::last_coxed_dates` (in `crate::rower::queries`) already
//!   reads from this table for S6's cox-cooldown signal.
//! - `recent_placements` returns the (practice_date, boat_id, seat,
//!   rower) tuples for the last N practices, which is what S7 (novelty)
//!   needs to detect repeated placements.

use crate::boat::types::BoatId;
use crate::practice::PracticeId;
use crate::rower::types::RowerId;
use crate::schema::{boat, lineup, lineup_seat, practice};
use crate::types::IntBool;
use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::SqliteConnection;
use serde::{Deserialize, Serialize};

/// Newtyped identifier for a `lineup` row. Matches the `BoatId` /
/// `RowerId` / `PracticeId` pattern.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct LineupId(i32);

impl LineupId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for LineupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for LineupId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Identifiable,
)]
#[diesel(table_name = crate::schema::lineup)]
pub struct Lineup {
    pub id: LineupId,
    pub practice_id: PracticeId,
    pub boat_id: BoatId,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::lineup)]
pub struct NewLineup {
    pub practice_id: PracticeId,
    pub boat_id: BoatId,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    diesel::Queryable,
    diesel::Selectable,
)]
#[diesel(table_name = crate::schema::lineup_seat)]
pub struct LineupSeatRow {
    pub lineup_id: LineupId,
    pub seat_position: i32,
    pub rower_id: RowerId,
    pub is_cox: IntBool,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::lineup_seat)]
pub struct NewLineupSeat {
    pub lineup_id: LineupId,
    pub seat_position: i32,
    pub rower_id: RowerId,
    pub is_cox: IntBool,
}

/// Input value type for `Lineup::commit_for_boat`. Decouples the
/// caller's ProposedLineup representation from this module's internal
/// row types so `lineup_db` doesn't need to depend on `lineup_solver`.
#[derive(Debug, Clone, Copy)]
pub struct CommitSeat {
    pub seat_position: i32,
    pub rower_id: RowerId,
    pub is_cox: bool,
}

/// A committed lineup paired with its seat rows, used by history
/// readers (S7 novelty, reports).
#[derive(Debug, Clone)]
pub struct CommittedLineup {
    pub lineup: Lineup,
    pub seats: Vec<LineupSeatRow>,
}

/// A flattened "who rowed where" record used by S7. Joins across
/// practice, lineup, and lineup_seat in one shot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentPlacement {
    pub practice_date: NaiveDate,
    pub boat_id: BoatId,
    pub seat_position: i32,
    pub rower_id: RowerId,
    pub is_cox: bool,
}

impl Lineup {
    /// Commit a lineup for one boat at one practice. If a lineup
    /// already exists for this `(practice_id, boat_id)` pair, it is
    /// deleted first (along with its seat rows via `ON DELETE CASCADE`)
    /// and replaced with the new one. This makes re-solve + re-commit
    /// cycles safely idempotent.
    ///
    /// The whole operation runs inside a transaction so a failed seat
    /// insert rolls back the lineup header.
    #[tracing::instrument(level = "debug", skip(conn, seats), err)]
    pub fn commit_for_boat(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
        boat_id: BoatId,
        seats: &[CommitSeat],
    ) -> Result<Lineup, diesel::result::Error> {
        let seats_owned: Vec<CommitSeat> = seats.to_vec();
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            // Delete any existing lineup for (practice, boat). ON DELETE
            // CASCADE on lineup_seat drops its rows too.
            diesel::delete(
                lineup::table
                    .filter(lineup::practice_id.eq(practice_id))
                    .filter(lineup::boat_id.eq(boat_id)),
            )
            .execute(conn)?;

            let now = chrono::Utc::now().naive_utc();
            let header: Lineup = diesel::insert_into(lineup::table)
                .values(NewLineup {
                    practice_id,
                    boat_id,
                    created_at: now,
                })
                .returning(Lineup::as_returning())
                .get_result(conn)?;

            let rows: Vec<NewLineupSeat> = seats_owned
                .into_iter()
                .map(|s| NewLineupSeat {
                    lineup_id: header.id,
                    seat_position: s.seat_position,
                    rower_id: s.rower_id,
                    is_cox: IntBool::new(s.is_cox),
                })
                .collect();
            if !rows.is_empty() {
                diesel::insert_into(lineup_seat::table)
                    .values(&rows)
                    .execute(conn)?;
            }

            Ok(header)
        })
    }

    /// All committed lineups for a given practice, with their seat rows.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn for_practice(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<Vec<CommittedLineup>, diesel::result::Error> {
        let headers: Vec<Lineup> = lineup::table
            .filter(lineup::practice_id.eq(practice_id))
            .select(Lineup::as_select())
            .order(lineup::id.asc())
            .get_results(conn)?;

        let mut out = Vec::with_capacity(headers.len());
        for header in headers {
            let seats: Vec<LineupSeatRow> = lineup_seat::table
                .filter(lineup_seat::lineup_id.eq(header.id))
                .select(LineupSeatRow::as_select())
                .order(lineup_seat::seat_position.asc())
                .get_results(conn)?;
            out.push(CommittedLineup {
                lineup: header,
                seats,
            });
        }
        Ok(out)
    }

    /// Flattened placements across the last `limit` practices, newest
    /// first. Used by S7 (novelty) to detect repeat `(rower, boat,
    /// seat)` triples and by reports to show "who rowed where recently".
    ///
    /// `limit` is a count of practices, not rows. If `limit = 4`, this
    /// returns every placement from the four most recent practices that
    /// have any committed lineups.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn recent_placements(
        conn: &mut SqliteConnection,
        limit: i64,
    ) -> Result<Vec<RecentPlacement>, diesel::result::Error> {
        // First: the N newest practice dates that have committed
        // lineups. `DISTINCT` + `ORDER BY date DESC LIMIT N`.
        let recent_dates: Vec<NaiveDate> = practice::table
            .inner_join(lineup::table.on(lineup::practice_id.eq(practice::id)))
            .select(practice::date)
            .distinct()
            .order(practice::date.desc())
            .limit(limit)
            .get_results(conn)?;

        if recent_dates.is_empty() {
            return Ok(vec![]);
        }

        // Then: flatten across (practice, lineup, lineup_seat, boat) for
        // those dates only. We pull through boat join to get the BoatId
        // typed correctly from the lineup table itself.
        let rows: Vec<(NaiveDate, BoatId, i32, RowerId, IntBool)> = practice::table
            .inner_join(lineup::table.on(lineup::practice_id.eq(practice::id)))
            .inner_join(lineup_seat::table.on(lineup_seat::lineup_id.eq(lineup::id)))
            .inner_join(boat::table.on(boat::id.eq(lineup::boat_id)))
            .filter(practice::date.eq_any(&recent_dates))
            .select((
                practice::date,
                lineup::boat_id,
                lineup_seat::seat_position,
                lineup_seat::rower_id,
                lineup_seat::is_cox,
            ))
            .order((practice::date.desc(), lineup::id.asc(), lineup_seat::seat_position.asc()))
            .get_results(conn)?;

        Ok(rows
            .into_iter()
            .map(|(date, boat_id, seat, rower, is_cox)| RecentPlacement {
                practice_date: date,
                boat_id,
                seat_position: seat,
                rower_id: rower,
                is_cox: is_cox.as_bool(),
            })
            .collect())
    }

    /// For a set of practice IDs, return the rower IDs placed in committed
    /// lineups. Used by the history list to detect stale lineups at a glance.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn committed_rower_ids_for_practices(
        conn: &mut SqliteConnection,
        practice_ids: &[PracticeId],
    ) -> Result<std::collections::HashMap<PracticeId, Vec<RowerId>>, diesel::result::Error> {
        let rows: Vec<(PracticeId, RowerId)> = lineup::table
            .inner_join(lineup_seat::table.on(lineup_seat::lineup_id.eq(lineup::id)))
            .filter(lineup::practice_id.eq_any(practice_ids))
            .select((lineup::practice_id, lineup_seat::rower_id))
            .get_results(conn)?;

        let mut map: std::collections::HashMap<PracticeId, Vec<RowerId>> =
            std::collections::HashMap::new();
        for (pid, rid) in rows {
            map.entry(pid).or_default().push(rid);
        }
        Ok(map)
    }

    /// Check whether a specific rower is placed in any committed lineup
    /// for a given practice.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn is_rower_in_committed_lineup(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
        rower_id: RowerId,
    ) -> Result<bool, diesel::result::Error> {
        let count: i64 = lineup::table
            .inner_join(lineup_seat::table.on(lineup_seat::lineup_id.eq(lineup::id)))
            .filter(lineup::practice_id.eq(practice_id))
            .filter(lineup_seat::rower_id.eq(rower_id))
            .count()
            .get_result(conn)?;
        Ok(count > 0)
    }
}
