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
pub struct RowerId(i32);

impl RowerId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for RowerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for RowerId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

/// Rower weight class. Intentionally coarser than `boat::types::WeightClass`
/// (no `Tubby` bucket) — individual rowers don't map onto the fourth boat
/// bucket cleanly.
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
pub enum RowerWeightClass {
    Light,
    Medium,
    Heavy,
}

impl RowerWeightClass {
    /// Ordinal used by the solver for weight-class average constraints.
    /// Higher = heavier. The values are chosen so that a unit of tolerance
    /// in the sum corresponds to one rower being off by exactly one class.
    pub fn ordinal(self) -> i32 {
        match self {
            Self::Light => 1,
            Self::Medium => 2,
            Self::Heavy => 3,
        }
    }
}

impl std::fmt::Display for RowerWeightClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Light => "Light",
            Self::Medium => "Medium",
            Self::Heavy => "Heavy",
        })
    }
}

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
pub enum Skill {
    Novice,
    Intermediate,
    Master,
    Expert,
}

impl Skill {
    /// Ordinal used by the solver for skill-variance soft constraints.
    /// Starts at 1 rather than 0 so the solver never has to multiply a
    /// decision variable by zero — Pumpkin panics on `.scaled(0)`. Spread
    /// is `max - min`, which is invariant under this shift.
    pub fn ordinal(self) -> i32 {
        match self {
            Self::Novice => 1,
            Self::Intermediate => 2,
            Self::Master => 3,
            Self::Expert => 4,
        }
    }
}

impl std::fmt::Display for Skill {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Novice => "Novice",
            Self::Intermediate => "Intermediate",
            Self::Master => "Master",
            Self::Expert => "Expert",
        })
    }
}

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
pub enum Strength {
    Weak,
    Intermediate,
    Strong,
    VeryStrong,
}

impl Strength {
    /// Ordinal used by the solver for pair-strength balance and skill /
    /// strength variance. Starts at 1 rather than 0 so the solver never
    /// multiplies a decision variable by zero — Pumpkin panics on
    /// `.scaled(0)`. Differences and variances are invariant under the
    /// shift.
    pub fn ordinal(self) -> i32 {
        match self {
            Self::Weak => 1,
            Self::Intermediate => 2,
            Self::Strong => 3,
            Self::VeryStrong => 4,
        }
    }
}

impl std::fmt::Display for Strength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Weak => "Weak",
            Self::Intermediate => "Intermediate",
            Self::Strong => "Strong",
            Self::VeryStrong => "VeryStrong",
        })
    }
}

/// Which side of the boat a rower rows on (in sweep).
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
pub enum Side {
    Port,
    Starboard,
    Either,
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Port => "Port",
            Self::Starboard => "Starboard",
            Self::Either => "Either",
        })
    }
}
