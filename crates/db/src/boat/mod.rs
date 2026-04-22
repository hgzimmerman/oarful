pub mod queries;
pub mod types;

use types::{BoatId, CoxPosition, OarsPerSeat, SeatCount, WeightClass};

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
    pub seat_count: SeatCount,
    pub has_cox: IntBool,
    pub oars_per_seat: OarsPerSeat,
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
    pub seat_count: SeatCount,
    pub has_cox: IntBool,
    pub oars_per_seat: OarsPerSeat,
    pub acquired_at: Option<chrono::NaiveDate>,
    pub manufactured_at: Option<chrono::NaiveDate>,
    pub stroke_side: Side,
    pub cox_position: CoxPosition,
}

impl Boat {
    /// True for sweep boats (one oar per seat). This project only generates
    /// lineups for sweep boats; sculling boats belong to the scullers team.
    pub fn is_sweep(&self) -> bool {
        self.oars_per_seat == OarsPerSeat::new(1)
    }

    pub fn in_service(&self) -> bool {
        self.relinquished_at.is_none()
    }

    pub fn is_scull(&self) -> bool {
        self.oars_per_seat == OarsPerSeat::new(2)
    }

    /// Which side of the boat a given seat sits on. Returns `None` for:
    /// - the cox seat (position 0)
    /// - all seats in scull boats (rowers use two oars, no side distinction)
    ///
    /// Standard alternating rig (sweep only): the stroke seat (position
    /// `seat_count`) sits on `stroke_side`; every seat one closer to the
    /// bow flips sides.
    pub fn seat_side(&self, seat: i32) -> Option<Side> {
        let sc = self.seat_count.as_int();
        if seat <= 0 || seat > sc {
            return None;
        }
        // Scull boats have no side distinction.
        if self.is_scull() {
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
        let distance_from_stroke = sc - seat;
        Some(if distance_from_stroke % 2 == 0 {
            self.stroke_side
        } else {
            opposite
        })
    }
}
