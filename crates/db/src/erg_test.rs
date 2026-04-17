//! Erg test log — records of ergometer test results per rower.
//!
//! Each entry records a time over a distance. Teams may use different
//! test distances at different times of year (2k, 5k, 6k, 1k). The
//! most recent entry per rower per distance is considered "current".

use crate::rower::types::RowerId;
use crate::schema::erg_test;
use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use diesel::SqliteConnection;

#[derive(Debug, Clone, diesel::Queryable, diesel::Selectable, diesel::Identifiable)]
#[diesel(table_name = erg_test)]
pub struct ErgTest {
    pub id: i32,
    pub rower_id: RowerId,
    /// Test distance in metres (e.g. 2000, 5000, 6000, 1000).
    pub distance_m: i32,
    /// Time in centiseconds (e.g. 42350 = 7:03.50).
    pub time_cs: i32,
    /// When the test was actually rowed (optional).
    pub rowed_at: Option<NaiveDate>,
    /// When the entry was recorded in the system.
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = erg_test)]
pub struct NewErgTest {
    pub rower_id: RowerId,
    pub distance_m: i32,
    pub time_cs: i32,
    pub rowed_at: Option<NaiveDate>,
    pub created_at: NaiveDateTime,
}

impl ErgTest {
    /// All erg tests for a rower, most recent first.
    pub fn list_for_rower(
        conn: &mut SqliteConnection,
        rower_id: RowerId,
    ) -> Result<Vec<ErgTest>, diesel::result::Error> {
        erg_test::table
            .filter(erg_test::rower_id.eq(rower_id))
            .select(ErgTest::as_select())
            .order(erg_test::created_at.desc())
            .get_results(conn)
    }

    /// Insert a new erg test entry.
    pub fn create(
        conn: &mut SqliteConnection,
        new: NewErgTest,
    ) -> Result<ErgTest, diesel::result::Error> {
        diesel::insert_into(erg_test::table)
            .values(&new)
            .returning(ErgTest::as_returning())
            .get_result(conn)
    }

    /// Delete an erg test entry by ID.
    pub fn delete(conn: &mut SqliteConnection, id: i32) -> Result<(), diesel::result::Error> {
        diesel::delete(erg_test::table.find(id)).execute(conn)?;
        Ok(())
    }
}

/// Format centiseconds as `M:SS.dd` (e.g. 42350 → "7:03.50").
pub fn format_time_cs(cs: i32) -> String {
    let total_secs = cs / 100;
    let frac = cs % 100;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}:{secs:02}.{frac:02}")
}

/// Format distance in metres as a compact label (e.g. 2000 → "2k").
pub fn format_distance(m: i32) -> String {
    if m >= 1000 && m % 1000 == 0 {
        format!("{}k", m / 1000)
    } else {
        format!("{m}m")
    }
}

/// Format kg as lbs (1 kg ≈ 2.20462 lbs).
pub fn kg_to_lbs(kg: f64) -> f64 {
    kg * 2.20462
}

/// Format metres as feet and inches (e.g. 1.803 → "5'11\"").
pub fn metres_to_ft_in(m: f64) -> String {
    let total_inches = m * 39.3701;
    let feet = total_inches as i32 / 12;
    let inches = (total_inches.round() as i32) % 12;
    format!("{feet}'{inches}\"")
}
