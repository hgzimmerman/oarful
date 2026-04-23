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

/// Rower weight class. Four tiers matching the 3-caret threshold UI.
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
    VeryHeavy,
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
            Self::VeryHeavy => 4,
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
            Self::VeryHeavy => "VH",
        }
    }
}

impl std::fmt::Display for RowerWeightClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Light => "Lightweight",
            Self::Medium => "Middleweight",
            Self::Heavy => "Heavyweight",
            Self::VeryHeavy => "Very heavy",
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
            Self::VeryTall => "Very tall",
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

/// How strongly a rower prefers sweep vs sculling. Stored as an
/// integer in [-2, 2]:
///
/// | Value | Meaning                    |
/// |-------|----------------------------|
/// | -2    | Hard sculler (never sweep) |
/// | -1    | Prefers sculling           |
/// |  0    | No preference              |
/// |  1    | Prefers sweeping           |
/// |  2    | Sweep only (never scull)   |
///
/// Replaces the old boolean `can_scull` flag. The solver uses this
/// to scale the S13 retention reward and to filter hard scullers
/// out of the sweep candidate pool entirely.
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
pub struct SweepBias(i32);

impl SweepBias {
    pub const SWEEP_HARD: Self = Self(2);
    pub const SCULL_HARD: Self = Self(-2);

    pub fn new(n: i32) -> Self {
        Self(n.clamp(-2, 2))
    }

    pub fn as_int(self) -> i32 {
        self.0
    }

    /// True if the rower is a hard sculler and should never be
    /// considered for sweep seating.
    pub fn is_hard_sculler(self) -> bool {
        self.0 == -2
    }
}

impl Default for SweepBias {
    fn default() -> Self {
        Self(0)
    }
}

impl std::fmt::Display for SweepBias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            -2 => f.write_str("Scull only"),
            -1 => f.write_str("Prefers scull"),
            0 => f.write_str("No preference"),
            1 => f.write_str("Prefers sweep"),
            2 => f.write_str("Sweep only"),
            n => write!(f, "{n}"),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── SideStrength ────────────────────────────────────────────────

    #[test]
    fn side_strength_hard() {
        assert!(SideStrength::HARD.is_hard());
        assert_eq!(SideStrength::HARD.as_int(), 0);
    }

    #[test]
    fn side_strength_soft_clamps() {
        assert_eq!(SideStrength::soft(0).as_int(), 1); // clamps up
        assert_eq!(SideStrength::soft(6).as_int(), 5); // clamps down
        assert_eq!(SideStrength::soft(3).as_int(), 3); // in range
    }

    #[test]
    fn side_strength_new_clamps() {
        assert_eq!(SideStrength::new(-5).as_int(), 0); // clamps up
        assert_eq!(SideStrength::new(10).as_int(), 5); // clamps down
        assert_eq!(SideStrength::new(0).as_int(), 0); // allowed via new()
    }

    #[test]
    fn side_strength_default() {
        assert_eq!(SideStrength::default().as_int(), 3);
        assert!(!SideStrength::default().is_hard());
    }

    // ── SweepBias ───────────────────────────────────────────────────

    #[test]
    fn sweep_bias_constants() {
        assert_eq!(SweepBias::SWEEP_HARD.as_int(), 2);
        assert_eq!(SweepBias::SCULL_HARD.as_int(), -2);
    }

    #[test]
    fn sweep_bias_clamps() {
        assert_eq!(SweepBias::new(5).as_int(), 2);
        assert_eq!(SweepBias::new(-5).as_int(), -2);
        assert_eq!(SweepBias::new(0).as_int(), 0);
    }

    #[test]
    fn sweep_bias_is_hard_sculler() {
        assert!(SweepBias::SCULL_HARD.is_hard_sculler());
        assert!(!SweepBias::new(-1).is_hard_sculler());
        assert!(!SweepBias::default().is_hard_sculler());
        assert!(!SweepBias::SWEEP_HARD.is_hard_sculler());
    }

    #[test]
    fn sweep_bias_default() {
        assert_eq!(SweepBias::default().as_int(), 0);
    }

    #[test]
    fn sweep_bias_display() {
        assert_eq!(SweepBias::SCULL_HARD.to_string(), "Scull only");
        assert_eq!(SweepBias::new(-1).to_string(), "Prefers scull");
        assert_eq!(SweepBias::default().to_string(), "No preference");
        assert_eq!(SweepBias::new(1).to_string(), "Prefers sweep");
        assert_eq!(SweepBias::SWEEP_HARD.to_string(), "Sweep only");
    }

    // ── Enum ordinals ───────────────────────────────────────────────

    #[test]
    fn rower_weight_class_ordinals_monotonic() {
        assert!(RowerWeightClass::Light.ordinal() < RowerWeightClass::Medium.ordinal());
        assert!(RowerWeightClass::Medium.ordinal() < RowerWeightClass::Heavy.ordinal());
        assert!(RowerWeightClass::Heavy.ordinal() < RowerWeightClass::VeryHeavy.ordinal());
    }

    #[test]
    fn skill_ordinals_monotonic() {
        assert!(Skill::Novice.ordinal() < Skill::Intermediate.ordinal());
        assert!(Skill::Intermediate.ordinal() < Skill::Master.ordinal());
        assert!(Skill::Master.ordinal() < Skill::Expert.ordinal());
    }

    #[test]
    fn strength_ordinals_monotonic() {
        assert!(Strength::Weak.ordinal() < Strength::Intermediate.ordinal());
        assert!(Strength::Intermediate.ordinal() < Strength::Strong.ordinal());
        assert!(Strength::Strong.ordinal() < Strength::VeryStrong.ordinal());
    }

    #[test]
    fn height_ordinals_monotonic() {
        assert!(Height::Short.ordinal() < Height::Medium.ordinal());
        assert!(Height::Medium.ordinal() < Height::Tall.ordinal());
        assert!(Height::Tall.ordinal() < Height::VeryTall.ordinal());
    }

    #[test]
    fn ordinals_start_at_one() {
        // Important: ordinals start at 1 to avoid Pumpkin .scaled(0) panics
        assert_eq!(RowerWeightClass::Light.ordinal(), 1);
        assert_eq!(Skill::Novice.ordinal(), 1);
        assert_eq!(Strength::Weak.ordinal(), 1);
        assert_eq!(Height::Short.ordinal(), 1);
    }

    // ── Enum short labels ───────────────────────────────────────────

    #[test]
    fn weight_class_short() {
        assert_eq!(RowerWeightClass::Light.short(), "Lt");
        assert_eq!(RowerWeightClass::VeryHeavy.short(), "VH");
    }

    #[test]
    fn skill_short() {
        assert_eq!(Skill::Novice.short(), "Nov");
        assert_eq!(Skill::Expert.short(), "Exp");
    }

    #[test]
    fn strength_short() {
        assert_eq!(Strength::Weak.short(), "Wk");
        assert_eq!(Strength::VeryStrong.short(), "V.Str");
    }

    // ── RowerId ─────────────────────────────────────────────────────

    #[test]
    fn rower_id_round_trip() {
        let id = RowerId::new(7);
        assert_eq!(id.as_int(), 7);
        assert_eq!(id.to_string(), "7");
        assert_eq!("7".parse::<RowerId>().unwrap(), id);
    }
}
