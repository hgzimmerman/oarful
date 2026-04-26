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

/// Typed seat position within a lineup. Position 0 is the cox seat on
/// coxed boats; 1..=seat_count are the rowing seats (bow → stroke).
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
pub struct SeatPosition(i32);

impl SeatPosition {
    pub fn new(n: i32) -> Self {
        Self(n)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for SeatPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for SeatPosition {
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
    pub is_draft: IntBool,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::lineup)]
pub struct NewLineup {
    pub practice_id: PracticeId,
    pub boat_id: BoatId,
    pub created_at: chrono::NaiveDateTime,
    pub is_draft: IntBool,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, diesel::Queryable, diesel::Selectable,
)]
#[diesel(table_name = crate::schema::lineup_seat)]
pub struct LineupSeatRow {
    pub lineup_id: LineupId,
    pub seat_position: SeatPosition,
    pub rower_id: RowerId,
    pub is_cox: IntBool,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::lineup_seat)]
pub struct NewLineupSeat {
    pub lineup_id: LineupId,
    pub seat_position: SeatPosition,
    pub rower_id: RowerId,
    pub is_cox: IntBool,
}

/// Input value type for `Lineup::commit_for_boat`. Decouples the
/// caller's ProposedLineup representation from this module's internal
/// row types so `lineup_db` doesn't need to depend on `lineup_solver`.
#[derive(Debug, Clone, Copy)]
pub struct CommitSeat {
    pub seat_position: SeatPosition,
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
    pub seat_position: SeatPosition,
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
                    is_draft: IntBool::FALSE,
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

    /// All committed (non-draft) lineups for a given practice, with their seat rows.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn for_practice(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<Vec<CommittedLineup>, diesel::result::Error> {
        let headers: Vec<Lineup> = lineup::table
            .filter(lineup::practice_id.eq(practice_id))
            .filter(lineup::is_draft.eq(0))
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
            .filter(lineup::is_draft.eq(0))
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
        let rows: Vec<(NaiveDate, BoatId, SeatPosition, RowerId, IntBool)> = practice::table
            .inner_join(lineup::table.on(lineup::practice_id.eq(practice::id)))
            .inner_join(lineup_seat::table.on(lineup_seat::lineup_id.eq(lineup::id)))
            .inner_join(boat::table.on(boat::id.eq(lineup::boat_id)))
            .filter(practice::date.eq_any(&recent_dates))
            .filter(lineup::is_draft.eq(0))
            .select((
                practice::date,
                lineup::boat_id,
                lineup_seat::seat_position,
                lineup_seat::rower_id,
                lineup_seat::is_cox,
            ))
            .order((
                practice::date.desc(),
                lineup::id.asc(),
                lineup_seat::seat_position.asc(),
            ))
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
            .filter(lineup::is_draft.eq(0))
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
            .filter(lineup::is_draft.eq(0))
            .filter(lineup_seat::rower_id.eq(rower_id))
            .count()
            .get_result(conn)?;
        Ok(count > 0)
    }

    /// Save a draft for an entire practice. Replaces any existing draft
    /// lineups for this practice (but does not touch committed ones).
    #[tracing::instrument(level = "debug", skip(conn, boats), err)]
    pub fn save_draft_for_practice(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
        boats: &[(BoatId, Vec<CommitSeat>)],
    ) -> Result<(), diesel::result::Error> {
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            // Delete existing drafts for this practice.
            diesel::delete(
                lineup::table
                    .filter(lineup::practice_id.eq(practice_id))
                    .filter(lineup::is_draft.eq(1)),
            )
            .execute(conn)?;

            let now = chrono::Utc::now().naive_utc();
            for (boat_id, seats) in boats {
                let header: Lineup = diesel::insert_into(lineup::table)
                    .values(NewLineup {
                        practice_id,
                        boat_id: *boat_id,
                        created_at: now,
                        is_draft: IntBool::TRUE,
                    })
                    .returning(Lineup::as_returning())
                    .get_result(conn)?;

                let rows: Vec<NewLineupSeat> = seats
                    .iter()
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
            }
            Ok(())
        })
    }

    /// All draft lineups for a given practice, with their seat rows.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn draft_for_practice(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<Vec<CommittedLineup>, diesel::result::Error> {
        let headers: Vec<Lineup> = lineup::table
            .filter(lineup::practice_id.eq(practice_id))
            .filter(lineup::is_draft.eq(1))
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

    /// Delete all draft lineups for a practice.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn delete_draft_for_practice(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<usize, diesel::result::Error> {
        diesel::delete(
            lineup::table
                .filter(lineup::practice_id.eq(practice_id))
                .filter(lineup::is_draft.eq(1)),
        )
        .execute(conn)
    }

    /// Whether a practice has any draft lineups.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn has_draft(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<bool, diesel::result::Error> {
        let count: i64 = lineup::table
            .filter(lineup::practice_id.eq(practice_id))
            .filter(lineup::is_draft.eq(1))
            .count()
            .get_result(conn)?;
        Ok(count > 0)
    }

    /// Which of the given practice IDs have at least one draft lineup?
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn practices_with_drafts(
        conn: &mut SqliteConnection,
        practice_ids: &[PracticeId],
    ) -> Result<std::collections::HashSet<PracticeId>, diesel::result::Error> {
        let ids: Vec<PracticeId> = lineup::table
            .filter(lineup::practice_id.eq_any(practice_ids))
            .filter(lineup::is_draft.eq(1))
            .select(lineup::practice_id)
            .distinct()
            .get_results(conn)?;
        Ok(ids.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boat::types::{CoxPosition, OarsPerSeat, SeatCount, WeightClass};
    use crate::boat::{Boat, NewBoat};
    use crate::practice::Practice;
    use crate::rower::types::{
        Height, RowerWeightClass, Side, SideStrength, Skill, Strength, SweepBias,
    };
    use crate::rower::{NewRower, Rower};
    use crate::team::{NewTeam, Team, TeamId};
    use crate::test_support::in_memory_conn;

    fn seed_team(conn: &mut diesel::SqliteConnection) -> TeamId {
        let now = chrono::Utc::now().naive_utc();
        Team::create(
            conn,
            NewTeam {
                name: "Test".into(),
                created_at: now,
            },
        )
        .unwrap()
        .id
    }

    fn seed_boat(conn: &mut diesel::SqliteConnection) -> Boat {
        Boat::insert(
            conn,
            NewBoat {
                name: "Eight".into(),
                weight_class: WeightClass::Heavy,
                seat_count: SeatCount::new(8),
                has_cox: IntBool::TRUE,
                oars_per_seat: OarsPerSeat::new(1),
                acquired_at: None,
                manufactured_at: None,
                stroke_side: Side::Starboard,
                cox_position: CoxPosition::Stern,
            },
        )
        .unwrap()
    }

    fn seed_rower(conn: &mut diesel::SqliteConnection, name: &str) -> Rower {
        let now = chrono::Utc::now().naive_utc();
        Rower::insert(
            conn,
            NewRower {
                name: name.into(),
                weight_class: RowerWeightClass::Medium,
                skill: Skill::Intermediate,
                strength: Strength::Intermediate,
                height: Height::Medium,
                side: Side::Port,
                side_strength: SideStrength::default(),
                sweep_bias: SweepBias::default(),
                can_cox: IntBool::TRUE,
                is_designated_cox: IntBool::FALSE,
                active: IntBool::TRUE,
                created_at: now,
                updated_at: now,
            },
        )
        .unwrap()
    }

    fn seed_practice(
        conn: &mut diesel::SqliteConnection,
        tid: TeamId,
        date: NaiveDate,
    ) -> Practice {
        Practice::upsert(conn, tid, date, None, None).unwrap()
    }

    #[test]
    fn commit_and_retrieve() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let boat = seed_boat(&mut conn);
        let r1 = seed_rower(&mut conn, "R1");
        let r2 = seed_rower(&mut conn, "R2");
        let practice = seed_practice(&mut conn, tid, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());

        let lineup = Lineup::commit_for_boat(
            &mut conn,
            practice.id,
            boat.id,
            &[
                CommitSeat {
                    seat_position: SeatPosition::new(1),
                    rower_id: r1.id,
                    is_cox: false,
                },
                CommitSeat {
                    seat_position: SeatPosition::new(2),
                    rower_id: r2.id,
                    is_cox: false,
                },
            ],
        )
        .unwrap();

        let committed = Lineup::for_practice(&mut conn, practice.id).unwrap();
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].lineup.id, lineup.id);
        assert_eq!(committed[0].seats.len(), 2);
    }

    #[test]
    fn commit_replaces_existing() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let boat = seed_boat(&mut conn);
        let r1 = seed_rower(&mut conn, "R1");
        let r2 = seed_rower(&mut conn, "R2");
        let practice = seed_practice(&mut conn, tid, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());

        // First commit
        Lineup::commit_for_boat(
            &mut conn,
            practice.id,
            boat.id,
            &[CommitSeat {
                seat_position: SeatPosition::new(1),
                rower_id: r1.id,
                is_cox: false,
            }],
        )
        .unwrap();

        // Second commit replaces
        Lineup::commit_for_boat(
            &mut conn,
            practice.id,
            boat.id,
            &[CommitSeat {
                seat_position: SeatPosition::new(1),
                rower_id: r2.id,
                is_cox: false,
            }],
        )
        .unwrap();

        let committed = Lineup::for_practice(&mut conn, practice.id).unwrap();
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].seats[0].rower_id, r2.id);
    }

    #[test]
    fn is_rower_in_committed_lineup() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let boat = seed_boat(&mut conn);
        let r1 = seed_rower(&mut conn, "R1");
        let r2 = seed_rower(&mut conn, "R2");
        let practice = seed_practice(&mut conn, tid, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());

        Lineup::commit_for_boat(
            &mut conn,
            practice.id,
            boat.id,
            &[CommitSeat {
                seat_position: SeatPosition::new(1),
                rower_id: r1.id,
                is_cox: false,
            }],
        )
        .unwrap();

        assert!(Lineup::is_rower_in_committed_lineup(&mut conn, practice.id, r1.id).unwrap());
        assert!(!Lineup::is_rower_in_committed_lineup(&mut conn, practice.id, r2.id).unwrap());
    }

    #[test]
    fn recent_placements() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let boat = seed_boat(&mut conn);
        let r1 = seed_rower(&mut conn, "R1");

        for day in [1, 2, 3] {
            let p = seed_practice(
                &mut conn,
                tid,
                NaiveDate::from_ymd_opt(2026, 5, day).unwrap(),
            );
            Lineup::commit_for_boat(
                &mut conn,
                p.id,
                boat.id,
                &[CommitSeat {
                    seat_position: SeatPosition::new(1),
                    rower_id: r1.id,
                    is_cox: false,
                }],
            )
            .unwrap();
        }

        let placements = Lineup::recent_placements(&mut conn, 2).unwrap();
        // Should only include the 2 most recent practices
        let dates: Vec<_> = placements.iter().map(|p| p.practice_date).collect();
        assert_eq!(dates.len(), 2);
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 5, 3).unwrap()));
        assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()));
    }

    #[test]
    fn committed_rower_ids_for_practices() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let boat = seed_boat(&mut conn);
        let r1 = seed_rower(&mut conn, "R1");
        let r2 = seed_rower(&mut conn, "R2");
        let p1 = seed_practice(&mut conn, tid, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        let p2 = seed_practice(&mut conn, tid, NaiveDate::from_ymd_opt(2026, 5, 2).unwrap());

        Lineup::commit_for_boat(
            &mut conn,
            p1.id,
            boat.id,
            &[CommitSeat {
                seat_position: SeatPosition::new(1),
                rower_id: r1.id,
                is_cox: false,
            }],
        )
        .unwrap();
        Lineup::commit_for_boat(
            &mut conn,
            p2.id,
            boat.id,
            &[
                CommitSeat {
                    seat_position: SeatPosition::new(1),
                    rower_id: r1.id,
                    is_cox: false,
                },
                CommitSeat {
                    seat_position: SeatPosition::new(2),
                    rower_id: r2.id,
                    is_cox: false,
                },
            ],
        )
        .unwrap();

        let map = Lineup::committed_rower_ids_for_practices(&mut conn, &[p1.id, p2.id]).unwrap();
        assert_eq!(map[&p1.id].len(), 1);
        assert_eq!(map[&p2.id].len(), 2);
    }

    #[test]
    fn draft_not_visible_in_committed_queries() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let boat = seed_boat(&mut conn);
        let r1 = seed_rower(&mut conn, "R1");
        let practice = seed_practice(&mut conn, tid, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());

        // Save a draft
        Lineup::save_draft_for_practice(
            &mut conn,
            practice.id,
            &[(
                boat.id,
                vec![CommitSeat {
                    seat_position: SeatPosition::new(1),
                    rower_id: r1.id,
                    is_cox: false,
                }],
            )],
        )
        .unwrap();

        // Draft should not appear in committed queries
        assert!(Lineup::for_practice(&mut conn, practice.id)
            .unwrap()
            .is_empty());
        assert!(!Lineup::is_rower_in_committed_lineup(&mut conn, practice.id, r1.id).unwrap());
        assert!(Lineup::recent_placements(&mut conn, 10).unwrap().is_empty());
        let map = Lineup::committed_rower_ids_for_practices(&mut conn, &[practice.id]).unwrap();
        assert!(map.is_empty());

        // But draft_for_practice should return it
        let drafts = Lineup::draft_for_practice(&mut conn, practice.id).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].seats.len(), 1);
        assert_eq!(drafts[0].seats[0].rower_id, r1.id);

        // has_draft should be true
        assert!(Lineup::has_draft(&mut conn, practice.id).unwrap());
    }

    #[test]
    fn commit_replaces_draft() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let boat = seed_boat(&mut conn);
        let r1 = seed_rower(&mut conn, "R1");
        let r2 = seed_rower(&mut conn, "R2");
        let practice = seed_practice(&mut conn, tid, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());

        // Save a draft with r1
        Lineup::save_draft_for_practice(
            &mut conn,
            practice.id,
            &[(
                boat.id,
                vec![CommitSeat {
                    seat_position: SeatPosition::new(1),
                    rower_id: r1.id,
                    is_cox: false,
                }],
            )],
        )
        .unwrap();

        // Commit with r2 — should replace the draft
        Lineup::commit_for_boat(
            &mut conn,
            practice.id,
            boat.id,
            &[CommitSeat {
                seat_position: SeatPosition::new(1),
                rower_id: r2.id,
                is_cox: false,
            }],
        )
        .unwrap();

        // Draft should be gone, committed should have r2
        assert!(!Lineup::has_draft(&mut conn, practice.id).unwrap());
        let committed = Lineup::for_practice(&mut conn, practice.id).unwrap();
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].seats[0].rower_id, r2.id);
    }

    #[test]
    fn save_draft_replaces_previous_draft() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let boat = seed_boat(&mut conn);
        let r1 = seed_rower(&mut conn, "R1");
        let r2 = seed_rower(&mut conn, "R2");
        let practice = seed_practice(&mut conn, tid, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());

        // First draft with r1
        Lineup::save_draft_for_practice(
            &mut conn,
            practice.id,
            &[(
                boat.id,
                vec![CommitSeat {
                    seat_position: SeatPosition::new(1),
                    rower_id: r1.id,
                    is_cox: false,
                }],
            )],
        )
        .unwrap();

        // Second draft with r2 replaces
        Lineup::save_draft_for_practice(
            &mut conn,
            practice.id,
            &[(
                boat.id,
                vec![CommitSeat {
                    seat_position: SeatPosition::new(1),
                    rower_id: r2.id,
                    is_cox: false,
                }],
            )],
        )
        .unwrap();

        let drafts = Lineup::draft_for_practice(&mut conn, practice.id).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].seats[0].rower_id, r2.id);
    }

    #[test]
    fn delete_draft_preserves_committed() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let boat = seed_boat(&mut conn);
        let r1 = seed_rower(&mut conn, "R1");
        let r2 = seed_rower(&mut conn, "R2");
        let practice = seed_practice(&mut conn, tid, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());

        // Commit with r1
        Lineup::commit_for_boat(
            &mut conn,
            practice.id,
            boat.id,
            &[CommitSeat {
                seat_position: SeatPosition::new(1),
                rower_id: r1.id,
                is_cox: false,
            }],
        )
        .unwrap();

        // Also save a draft (different boat to avoid collision)
        let boat2 = Boat::insert(
            &mut conn,
            NewBoat {
                name: "Four".into(),
                weight_class: WeightClass::Heavy,
                seat_count: SeatCount::new(4),
                has_cox: IntBool::FALSE,
                oars_per_seat: OarsPerSeat::new(1),
                acquired_at: None,
                manufactured_at: None,
                stroke_side: Side::Starboard,
                cox_position: CoxPosition::Stern,
            },
        )
        .unwrap();

        Lineup::save_draft_for_practice(
            &mut conn,
            practice.id,
            &[(
                boat2.id,
                vec![CommitSeat {
                    seat_position: SeatPosition::new(1),
                    rower_id: r2.id,
                    is_cox: false,
                }],
            )],
        )
        .unwrap();

        // Delete draft
        Lineup::delete_draft_for_practice(&mut conn, practice.id).unwrap();

        // Draft gone, committed still there
        assert!(!Lineup::has_draft(&mut conn, practice.id).unwrap());
        let committed = Lineup::for_practice(&mut conn, practice.id).unwrap();
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].seats[0].rower_id, r1.id);
    }
}
