use crate::schema::{lineup, practice};
use crate::team::TeamId;
use crate::types::IntBool;
use chrono::NaiveDate;
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
    pub notes: Option<String>,
    pub cancelled: IntBool,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::practice)]
pub struct NewPractice {
    pub team_id: TeamId,
    pub date: NaiveDate,
    pub notes: Option<String>,
}

impl Practice {
    /// Find or create a practice for a (team, date) pair. If one
    /// already exists, returns it unchanged (notes are not overwritten).
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn upsert_by_date(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        date: NaiveDate,
        notes: Option<String>,
    ) -> Result<Practice, diesel::result::Error> {
        if let Some(existing) = practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::date.eq(date))
            .select(Practice::as_select())
            .first(conn)
            .optional()?
        {
            return Ok(existing);
        }
        diesel::insert_into(practice::table)
            .values(NewPractice {
                team_id,
                date,
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

    /// Which of the given dates have at least one committed lineup?
    /// Returns only the dates that match, as a set-friendly vec.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn committed_dates(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        dates: &[NaiveDate],
    ) -> Result<Vec<NaiveDate>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::date.eq_any(dates))
            .filter(practice::id.eq_any(lineup::table.select(lineup::practice_id)))
            .select(practice::date)
            .get_results(conn)
    }

    /// Update the notes on an existing practice row.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn update_notes(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        date: NaiveDate,
        notes: Option<String>,
    ) -> Result<Practice, diesel::result::Error> {
        diesel::update(
            practice::table
                .filter(practice::team_id.eq(team_id))
                .filter(practice::date.eq(date)),
        )
        .set(practice::notes.eq(notes))
        .returning(Practice::as_returning())
        .get_result(conn)
    }

    /// Future practice dates (on or after `today`), ordered ascending.
    /// Excludes cancelled practices.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_upcoming(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        today: NaiveDate,
    ) -> Result<Vec<NaiveDate>, diesel::result::Error> {
        practice::table
            .filter(practice::team_id.eq(team_id))
            .filter(practice::date.ge(today))
            .filter(practice::cancelled.eq(0))
            .select(practice::date)
            .order(practice::date.asc())
            .get_results(conn)
    }

    /// Toggle the cancelled flag on a practice.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn set_cancelled(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        date: NaiveDate,
        cancelled: bool,
    ) -> Result<Practice, diesel::result::Error> {
        diesel::update(
            practice::table
                .filter(practice::team_id.eq(team_id))
                .filter(practice::date.eq(date)),
        )
        .set(practice::cancelled.eq(if cancelled { 1 } else { 0 }))
        .returning(Practice::as_returning())
        .get_result(conn)
    }

    /// Find an existing practice for a (team, date) pair.
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
}
