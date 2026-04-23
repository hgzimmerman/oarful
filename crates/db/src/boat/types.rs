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
pub struct BoatId(i32);

impl BoatId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for BoatId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for BoatId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

/// Number of rowing seats (excludes cox). An 8+ has `SeatCount::new(8)`.
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
pub struct SeatCount(i32);

impl SeatCount {
    pub fn new(n: i32) -> Self {
        Self(n)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for SeatCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for SeatCount {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

/// Number of oars each rower uses: 1 for sweep, 2 for scull.
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
pub struct OarsPerSeat(i32);

impl OarsPerSeat {
    pub fn new(n: i32) -> Self {
        Self(n)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for OarsPerSeat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for OarsPerSeat {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

/// Boat weight class — which range of rower body-weights the boat is rigged
/// for. Note this is NOT the same enum as [`crate::rower::types::RowerWeightClass`];
/// boats have a `Tubby` bucket for very-heavy-crew boats that rowers don't.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    diesel_derive_enum::DbEnum,
)]
#[DbValueStyle = "verbatim"]
pub enum WeightClass {
    Light,
    Medium,
    Heavy,
    Tubby,
}

impl std::fmt::Display for WeightClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            WeightClass::Light => "Light",
            WeightClass::Medium => "Medium",
            WeightClass::Heavy => "Heavy",
            WeightClass::Tubby => "Tubby",
        })
    }
}

/// Physical position of the coxswain seat. Determines display order
/// of seats in lineup cards: stern→bow means cox appears at the top
/// for stern-loaders and at the bottom for bow-loaders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, diesel_derive_enum::DbEnum)]
#[DbValueStyle = "verbatim"]
pub enum CoxPosition {
    Bow,
    Stern,
}

impl CoxPosition {
    /// Whether the cox should be displayed first (at the top of the
    /// lineup card). Stern-loaders show cox first; bow-loaders last.
    pub fn cox_first(&self) -> bool {
        matches!(self, CoxPosition::Stern)
    }
}

impl std::fmt::Display for CoxPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CoxPosition::Bow => "Bow",
            CoxPosition::Stern => "Stern",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── CoxPosition ─────────────────────────────────────────────────

    #[test]
    fn cox_position_stern_is_first() {
        assert!(CoxPosition::Stern.cox_first());
        assert!(!CoxPosition::Bow.cox_first());
    }

    #[test]
    fn cox_position_display() {
        assert_eq!(CoxPosition::Bow.to_string(), "Bow");
        assert_eq!(CoxPosition::Stern.to_string(), "Stern");
    }

    // ── WeightClass ─────────────────────────────────────────────────

    #[test]
    fn boat_weight_class_display() {
        assert_eq!(WeightClass::Light.to_string(), "Light");
        assert_eq!(WeightClass::Medium.to_string(), "Medium");
        assert_eq!(WeightClass::Heavy.to_string(), "Heavy");
        assert_eq!(WeightClass::Tubby.to_string(), "Tubby");
    }

    // ── ID newtypes ─────────────────────────────────────────────────

    #[test]
    fn boat_id_round_trip() {
        let id = BoatId::new(5);
        assert_eq!(id.as_int(), 5);
        assert_eq!(id.to_string(), "5");
        assert_eq!("5".parse::<BoatId>().unwrap(), id);
    }

    #[test]
    fn seat_count_round_trip() {
        let sc = SeatCount::new(8);
        assert_eq!(sc.as_int(), 8);
        assert_eq!(sc.to_string(), "8");
        assert_eq!("8".parse::<SeatCount>().unwrap(), sc);
    }

    #[test]
    fn oars_per_seat_round_trip() {
        let sweep = OarsPerSeat::new(1);
        let scull = OarsPerSeat::new(2);
        assert_eq!(sweep.as_int(), 1);
        assert_eq!(scull.as_int(), 2);
        assert_eq!("1".parse::<OarsPerSeat>().unwrap(), sweep);
    }
}
