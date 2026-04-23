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
use serde::{Deserialize, Serialize};

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
pub struct ErgTestId(i32);

impl ErgTestId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for ErgTestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for ErgTestId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

#[derive(Debug, Clone, diesel::Queryable, diesel::Selectable, diesel::Identifiable)]
#[diesel(table_name = erg_test)]
pub struct ErgTest {
    pub id: ErgTestId,
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
    pub fn delete(conn: &mut SqliteConnection, id: ErgTestId) -> Result<(), diesel::result::Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_time_cs ──────────────────────────────────────────────

    #[test]
    fn format_time_cs_typical_2k() {
        // 7:03.50 = 423.50s = 42350cs
        assert_eq!(format_time_cs(42350), "7:03.50");
    }

    #[test]
    fn format_time_cs_exact_minutes() {
        // 6:00.00 = 360s = 36000cs
        assert_eq!(format_time_cs(36000), "6:00.00");
    }

    #[test]
    fn format_time_cs_sub_minute() {
        // 0:45.12 = 45.12s = 4512cs
        assert_eq!(format_time_cs(4512), "0:45.12");
    }

    #[test]
    fn format_time_cs_single_digit_frac() {
        // 1:30.05 = 90.05s = 9005cs
        assert_eq!(format_time_cs(9005), "1:30.05");
    }

    // ── format_distance ─────────────────────────────────────────────

    #[test]
    fn format_distance_thousands() {
        assert_eq!(format_distance(2000), "2k");
        assert_eq!(format_distance(5000), "5k");
        assert_eq!(format_distance(6000), "6k");
        assert_eq!(format_distance(1000), "1k");
    }

    #[test]
    fn format_distance_non_thousands() {
        assert_eq!(format_distance(500), "500m");
        assert_eq!(format_distance(1500), "1500m");
        assert_eq!(format_distance(2500), "2500m");
    }

    #[test]
    fn format_distance_below_thousand() {
        assert_eq!(format_distance(100), "100m");
    }

    // ── kg_to_lbs ───────────────────────────────────────────────────

    #[test]
    fn kg_to_lbs_known_values() {
        assert!((kg_to_lbs(100.0) - 220.462).abs() < 0.01);
        assert!((kg_to_lbs(0.0)).abs() < 0.001);
        assert!((kg_to_lbs(1.0) - 2.20462).abs() < 0.001);
    }

    // ── metres_to_ft_in ─────────────────────────────────────────────

    #[test]
    fn metres_to_ft_in_known_heights() {
        assert_eq!(metres_to_ft_in(1.8288), "6'0\""); // exactly 6'0"
        assert_eq!(metres_to_ft_in(1.8034), "5'11\""); // ~5'11"
    }

    // ── ID newtype ──────────────────────────────────────────────────

    #[test]
    fn erg_test_id_round_trip() {
        let id = ErgTestId::new(42);
        assert_eq!(id.as_int(), 42);
        assert_eq!(id.to_string(), "42");
        assert_eq!("42".parse::<ErgTestId>().unwrap(), id);
    }
}
