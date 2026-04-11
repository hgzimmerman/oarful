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

impl RowerWeightClass {
    /// Abbreviated label for compact stats lines.
    pub fn short(&self) -> &'static str {
        match self {
            Self::Light => "Lt",
            Self::Medium => "Md",
            Self::Heavy => "Hv",
        }
    }
}

impl std::fmt::Display for RowerWeightClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Light => "Lightweight",
            Self::Medium => "Middleweight",
            Self::Heavy => "Heavyweight",
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

impl Skill {
    /// Abbreviated label for compact stats lines. Uses "Form" category
    /// name to disambiguate from Strength's "Intermediate".
    pub fn short(&self) -> &'static str {
        match self {
            Self::Novice => "Nov",
            Self::Intermediate => "Int",
            Self::Master => "Mst",
            Self::Expert => "Exp",
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

impl Strength {
    /// Abbreviated label for compact stats lines.
    pub fn short(&self) -> &'static str {
        match self {
            Self::Weak => "Wk",
            Self::Intermediate => "Int",
            Self::Strong => "Str",
            Self::VeryStrong => "V.Str",
        }
    }
}

impl std::fmt::Display for Strength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Weak => "Weak",
            Self::Intermediate => "Intermediate",
            Self::Strong => "Strong",
            Self::VeryStrong => "Very strong",
        })
    }
}

/// Rower height, bucketed. Used by the S10 pair-height-balance soft
/// constraint. Ordinal starts at 1 rather than 0 to keep Pumpkin
/// happy (`.scaled(0)` panics); differences are shift-invariant.
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
pub enum Height {
    Short,
    Medium,
    Tall,
    VeryTall,
}

impl Height {
    pub fn ordinal(self) -> i32 {
        match self {
            Self::Short => 1,
            Self::Medium => 2,
            Self::Tall => 3,
            Self::VeryTall => 4,
        }
    }
}

impl std::fmt::Display for Height {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Short => "Short",
            Self::Medium => "Medium",
            Self::Tall => "Tall",
            Self::VeryTall => "VeryTall",
        })
    }
}

/// Strength of a rower's side preference. 0 is a hard lock — they can
/// only row their preferred side. 1..=5 is a soft preference scaling
/// the S4 wrong-side objective penalty: higher strength = bigger
/// penalty when the solver seats them on the wrong side. Enforced at
/// the SQL level via `CHECK( side_strength BETWEEN 0 AND 5 )`.
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
pub struct SideStrength(i32);

impl SideStrength {
    /// Hard side lock — the rower cannot be placed on the opposite
    /// side at all. The solver's eligibility filter rejects mismatched
    /// placements outright, so no `x` variable is ever created.
    pub const HARD: Self = Self(0);

    /// Construct a soft-preference strength, clamped to the 1..=5
    /// range. Use [`SideStrength::HARD`] for the hard-lock case.
    pub fn soft(n: i32) -> Self {
        Self(n.clamp(1, 5))
    }

    /// Construct from a raw integer, clamping to the full 0..=5 range.
    /// Useful for fixture / admin UI code that already has a validated
    /// value in the right range.
    pub fn new(n: i32) -> Self {
        Self(n.clamp(0, 5))
    }

    /// Raw value for use as a scale factor in the S4 objective term.
    pub fn as_int(self) -> i32 {
        self.0
    }

    /// True for [`SideStrength::HARD`] — the rower is side-locked.
    pub fn is_hard(self) -> bool {
        self.0 == 0
    }
}

impl Default for SideStrength {
    fn default() -> Self {
        Self(3)
    }
}

impl std::fmt::Display for SideStrength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
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
