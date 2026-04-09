use crate::schema::practice;
use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::SqliteConnection;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Identifiable,
)]
#[diesel(table_name = crate::schema::practice)]
pub struct Practice {
    pub id: i32,
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
