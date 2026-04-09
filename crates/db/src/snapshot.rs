//! A point-in-time view of everything the solver needs for one practice date.

use crate::availability::{types::AvailabilityStatus, Availability};
use crate::boat::Boat;
use crate::rower::{types::RowerId, Rower};
use chrono::NaiveDate;
use diesel::SqliteConnection;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DbSnapshot {
    pub date: NaiveDate,
    pub rowers: Vec<Rower>,
    /// Availability status for each rower that explicitly responded. Rowers
    /// not in this map are treated as "unset" (effectively `No` by default).
    pub availability: HashMap<RowerId, AvailabilityStatus>,
    /// In-service sweep boats — the only candidates for lineup assignment in
    /// this project. Sculling boats belong to the scullers team and are
    /// deliberately excluded.
    pub sweep_boats: Vec<Boat>,
    /// Derived from `lineup_seat` history.
    pub last_coxed: HashMap<RowerId, NaiveDate>,
}

impl DbSnapshot {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn for_date(
        conn: &mut SqliteConnection,
        date: NaiveDate,
    ) -> Result<Self, diesel::result::Error> {
        Ok(Self {
            date,
            rowers: Rower::list_active(conn)?,
            availability: Availability::map_for_date(conn, date)?,
            sweep_boats: Boat::list_sweep(conn)?,
            last_coxed: Rower::last_coxed_dates(conn)?,
        })
    }

    pub fn available_rowers(&self) -> impl Iterator<Item = &Rower> {
        self.rowers.iter().filter(|r| {
            self.availability
                .get(&r.id)
                .map(|s| s.is_available_for_sweep())
                .unwrap_or(false)
        })
    }
}

impl std::fmt::Display for DbSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Lineup snapshot for {} ===", self.date)?;

        writeln!(f, "\nSweep boats ({})", self.sweep_boats.len())?;
        for b in &self.sweep_boats {
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
                .unwrap_or_else(|| "-".to_string());
            let last_cox = self
                .last_coxed
                .get(&r.id)
                .map(|d| d.to_string())
                .unwrap_or_else(|| "-".to_string());
            let mut flags = String::new();
            if r.can_cox.as_bool() {
                flags.push('C');
            }
            if r.is_designated_cox.as_bool() {
                flags.push('*');
            }
            if r.can_scull.as_bool() {
                flags.push('S');
            }
            writeln!(
                f,
                "  #{:<3} {:<20} {:<7} {:<13} {:<13} side={:<10}({}) {:<4} avail={:<13} last_cox={}",
                r.id,
                r.name,
                r.weight_class,
                r.skill,
                r.strength,
                r.side.to_string(),
                r.side_strength,
                flags,
                status,
                last_cox
            )?;
        }
        Ok(())
    }
}
