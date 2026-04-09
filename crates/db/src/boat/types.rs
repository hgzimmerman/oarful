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
