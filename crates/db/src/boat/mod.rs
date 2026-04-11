pub mod queries;
pub mod types;

use types::{BoatId, CoxPosition, WeightClass};

use crate::rower::types::Side;
use crate::types::IntBool;

/// A boat in the fleet. Schema mirrors `boat_tracking` so the two apps can
/// eventually share a fleet snapshot. In this project boats are a thin
/// reference entity — we care about them because `lineup` foreign-keys here,
/// not because we manage maintenance / usage.
///
/// `stroke_side` records which side the stroke seat (the highest-numbered
/// rowing seat) sits on — this is the standard rowing convention for
/// describing a boat's rig: "starboard rigged" means stroke is on starboard,
/// "port rigged" means stroke is on port. Seats alternate strictly back
/// toward bow from there. `stroke_side` is never `Side::Either`; the SQL
/// CHECK keeps that variant out.
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
    diesel::AsChangeset,
)]
#[diesel(table_name = crate::schema::boat)]
pub struct Boat {
    pub id: BoatId,
    pub name: String,
    pub weight_class: WeightClass,
    pub seat_count: i32,
    pub has_cox: IntBool,
    pub oars_per_seat: i32,
    pub acquired_at: Option<chrono::NaiveDate>,
    pub manufactured_at: Option<chrono::NaiveDate>,
    pub relinquished_at: Option<chrono::NaiveDate>,
    pub stroke_side: Side,
    pub cox_position: CoxPosition,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::boat)]
pub struct NewBoat {
    pub name: String,
    pub weight_class: WeightClass,
    pub seat_count: i32,
    pub has_cox: IntBool,
    pub oars_per_seat: i32,
    pub acquired_at: Option<chrono::NaiveDate>,
    pub manufactured_at: Option<chrono::NaiveDate>,
    pub stroke_side: Side,
    pub cox_position: CoxPosition,
}

impl Boat {
    /// True for sweep boats (one oar per seat). This project only generates
    /// lineups for sweep boats; sculling boats belong to the scullers team.
    pub fn is_sweep(&self) -> bool {
        self.oars_per_seat == 1
    }

    pub fn in_service(&self) -> bool {
        self.relinquished_at.is_none()
    }

    /// Which side of the boat a given seat sits on. Returns `None` for the
    /// cox seat (position 0) — cox has no side.
    ///
    /// Standard alternating rig: the stroke seat (position `seat_count`)
    /// sits on `stroke_side`; every seat one closer to the bow flips sides.
    /// Equivalently, two seats with the same parity as `seat_count` share
    /// `stroke_side`; the opposite parity sits on the other side.
    pub fn seat_side(&self, seat: i32) -> Option<Side> {
        if seat <= 0 || seat > self.seat_count {
            return None;
        }
        let opposite = match self.stroke_side {
            Side::Port => Side::Starboard,
            Side::Starboard => Side::Port,
            // Unreachable in practice — the SQL CHECK forbids Either on boats —
            // but be explicit rather than panic.
            Side::Either => Side::Either,
        };
        // Distance from the stroke seat. Even distance => same side as stroke.
        let distance_from_stroke = self.seat_count - seat;
        Some(if distance_from_stroke % 2 == 0 {
            self.stroke_side
        } else {
            opposite
        })
    }
}
