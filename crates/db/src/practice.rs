use crate::schema::{lineup, practice};
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
    pub date: NaiveDate,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::practice)]
pub struct NewPractice {
    pub date: NaiveDate,
    pub notes: Option<String>,
}

impl Practice {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn upsert_by_date(
        conn: &mut SqliteConnection,
        date: NaiveDate,
        notes: Option<String>,
    ) -> Result<Practice, diesel::result::Error> {
        if let Some(existing) = practice::table
            .filter(practice::date.eq(date))
            .select(Practice::as_select())
            .first(conn)
            .optional()?
        {
            return Ok(existing);
        }
        diesel::insert_into(practice::table)
            .values(NewPractice { date, notes })
            .returning(Practice::as_returning())
            .get_result(conn)
    }

    /// Practices that have at least one committed lineup, newest first.
    /// Used by the `/history` page. Filters via a subquery on
    /// `lineup.practice_id` so no join or DISTINCT is needed.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_committed(
        conn: &mut SqliteConnection,
    ) -> Result<Vec<Practice>, diesel::result::Error> {
        practice::table
            .filter(practice::id.eq_any(lineup::table.select(lineup::practice_id)))
            .select(Practice::as_select())
            .order(practice::date.desc())
            .get_results(conn)
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn find_by_date(
        conn: &mut SqliteConnection,
        date: NaiveDate,
    ) -> Result<Option<Practice>, diesel::result::Error> {
        practice::table
            .filter(practice::date.eq(date))
            .select(Practice::as_select())
            .first(conn)
            .optional()
    }
}
