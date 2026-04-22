use crate::schema::{lineup, practice};
use crate::team::TeamId;
use crate::types::{DurationMinutes, IntBool};
use chrono::{NaiveDate, NaiveTime};
use diesel::prelude::*;
use diesel::SqliteConnection;
use serde::{Deserialize, Serialize};

/// Newtyped identifier for a `practice` row. Transparent wrapper over
/// `i32` with `diesel_derive_newtype::DieselNewType` doing the column
/// glue. Matches the `BoatId` / `RowerId` pattern.
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
pub struct PracticeId(i32);

impl PracticeId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for PracticeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for PracticeId {
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
#[diesel(table_name = crate::schema::practice)]
pub struct Practice {
    pub id: PracticeId,
    pub team_id: TeamId,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub notes: Option<String>,
    pub cancelled: IntBool,
    /// Per-practice duration override. If None, falls back to the
    /// team's `default_practice_duration_minutes`.
    pub duration_minutes: Option<DurationMinutes>,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::practice)]
pub struct NewPractice {
    pub team_id: TeamId,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub notes: Option<String>,
}

impl Practice {
    /// Look up a practice by its primary key.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn get(
        conn: &mut SqliteConnection,
        id: PracticeId,
    ) -> Result<Option<Practice>, diesel::result::Error> {
        practice::table
            .find(id)
            .select(Practice::as_select())
            .first(conn)
            .optional()
    }

    /// Find or create a practice for a (team, date, time) triple. If one
    /// already exists, returns it unchanged (notes are not overwritten).
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn upsert(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        date: NaiveDate,
        time: Option<NaiveTime>,
        notes: Option<String>,
    ) -> Result<Practice, diesel::result::Error> {
        let mut query = practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::date.eq(date))
            .into_boxed();
        if let Some(t) = time {
            query = query.filter(practice::time.eq(t));
        } else {
            query = query.filter(practice::time.is_null());
        }
        if let Some(existing) = query.select(Practice::as_select()).first(conn).optional()? {
            return Ok(existing);
        }
        diesel::insert_into(practice::table)
            .values(NewPractice {
                team_id,
                date,
                time,
                notes,
            })
            .returning(Practice::as_returning())
            .get_result(conn)
    }

    /// Practices with at least one committed lineup, newest first.
    /// Scoped to a single team.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_committed(
        conn: &mut SqliteConnection,
        team_id: TeamId,
    ) -> Result<Vec<Practice>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::id.eq_any(lineup::table.select(lineup::practice_id)))
            .select(Practice::as_select())
            .order(practice::date.desc())
            .get_results(conn)
    }

    /// Which of the given practice IDs have at least one committed lineup?
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn committed_ids(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        ids: &[PracticeId],
    ) -> Result<Vec<PracticeId>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::id.eq_any(ids))
            .filter(practice::id.eq_any(lineup::table.select(lineup::practice_id)))
            .select(practice::id)
            .get_results(conn)
    }

    /// Update the notes on an existing practice row.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn update_notes_by_id(
        conn: &mut SqliteConnection,
        id: PracticeId,
        notes: Option<String>,
    ) -> Result<Practice, diesel::result::Error> {
        diesel::update(practice::table.find(id))
            .set(practice::notes.eq(notes))
            .returning(Practice::as_returning())
            .get_result(conn)
    }

    /// Future practices (on or after `today`), ordered ascending.
    /// Excludes cancelled practices.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_upcoming(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        today: NaiveDate,
    ) -> Result<Vec<Practice>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::date.ge(today))
            .filter(practice::cancelled.eq(0))
            .select(Practice::as_select())
            .order((practice::date.asc(), practice::time.asc()))
            .get_results(conn)
    }

    /// Toggle the cancelled flag on a practice by ID.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn set_cancelled_by_id(
        conn: &mut SqliteConnection,
        id: PracticeId,
        cancelled: bool,
    ) -> Result<Practice, diesel::result::Error> {
        diesel::update(practice::table.find(id))
            .set(practice::cancelled.eq(if cancelled { 1 } else { 0 }))
            .returning(Practice::as_returning())
            .get_result(conn)
    }

    /// Non-cancelled practices for a team on or after `since`,
    /// ordered ascending.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_since(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        since: NaiveDate,
    ) -> Result<Vec<Practice>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::date.ge(since))
            .filter(practice::cancelled.eq(0))
            .select(Practice::as_select())
            .order((practice::date.asc(), practice::time.asc()))
            .get_results(conn)
    }

    /// Find an existing practice for a (team, date) pair.
    /// When multiple practices exist on the same date, returns the first.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn find_by_date(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        date: NaiveDate,
    ) -> Result<Option<Practice>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::date.eq(date))
            .select(Practice::as_select())
            .first(conn)
            .optional()
    }

    /// Display label for a practice. Shows just the date when alone,
    /// or date + time when `show_time` is true (multiple on same day).
    pub fn label(&self) -> String {
        match self.time {
            Some(t) => format!("{} · {}", self.date.format("%b %-d"), t.format("%-I:%M %p")),
            None => self.date.format("%b %-d").to_string(),
        }
    }

    /// Effective duration in minutes: per-practice override, then
    /// team default, then None (unknown).
    pub fn effective_duration(
        &self,
        team_default: Option<DurationMinutes>,
    ) -> Option<DurationMinutes> {
        self.duration_minutes.or(team_default)
    }

    /// Compute the [start, end) time window for this practice.
    /// Returns None if either time or duration is unknown.
    pub fn time_window(
        &self,
        team_default_duration: Option<DurationMinutes>,
    ) -> Option<(NaiveTime, NaiveTime)> {
        let start = self.time?;
        let dur = self.effective_duration(team_default_duration)?;
        let end = start + chrono::TimeDelta::minutes(dur.as_int() as i64);
        Some((start, end))
    }

    /// Find non-cancelled practices on *other* teams that overlap this
    /// practice's time window on the same date. Returns an empty vec if
    /// this practice has no time or duration set.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn find_overlapping(
        conn: &mut SqliteConnection,
        this: &Practice,
        this_team_default_duration: Option<DurationMinutes>,
    ) -> Result<Vec<Practice>, diesel::result::Error> {
        let Some((my_start, my_end)) = this.time_window(this_team_default_duration) else {
            return Ok(Vec::new());
        };
        // Candidate practices: same date, different team, not cancelled, has a time.
        let candidates: Vec<Practice> = practice::table
            .filter(practice::date.eq(this.date))
            .filter(practice::team_id.ne(this.team_id))
            .filter(practice::cancelled.eq(0))
            .filter(practice::time.is_not_null())
            .select(Practice::as_select())
            .get_results(conn)?;

        // Filter to those with overlapping time windows. We need each
        // candidate's team default duration — load lazily per team.
        use std::collections::HashMap;
        let mut team_defaults: HashMap<TeamId, Option<DurationMinutes>> = HashMap::new();
        let mut overlapping = Vec::new();
        for p in candidates {
            let team_dur = match team_defaults.get(&p.team_id) {
                Some(d) => *d,
                None => {
                    let t = crate::team::Team::get(conn, p.team_id)?;
                    let d = t.and_then(|t| t.default_practice_duration_minutes);
                    team_defaults.insert(p.team_id, d);
                    d
                }
            };
            if let Some((their_start, their_end)) = p.time_window(team_dur) {
                // Overlap: my_start < their_end AND their_start < my_end
                if my_start < their_end && their_start < my_end {
                    overlapping.push(p);
                }
            }
        }
        Ok(overlapping)
    }

    /// Full label including year.
    pub fn label_full(&self) -> String {
        match self.time {
            Some(t) => format!(
                "{} · {}",
                self.date.format("%A, %B %-d, %Y"),
                t.format("%-I:%M %p")
            ),
            None => self.date.format("%A, %B %-d, %Y").to_string(),
        }
    }
}
