//! Crate-wide shared newtypes that don't belong to any single entity.

use serde::{Deserialize, Serialize};

/// An integer-backed boolean (0 / 1) stored in sqlite. Used for all the
/// `CHECK( x IN (0,1) )` columns: rower flags, lineup_seat.is_cox, etc.
///
/// This mirrors the `HasCox` pattern from boat_tracking but is generalised so
/// we don't have to newtype every bool column separately.
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
pub struct IntBool(pub i32);

impl IntBool {
    pub const TRUE: Self = Self(1);
    pub const FALSE: Self = Self(0);

    pub fn new(b: bool) -> Self {
        if b {
            Self::TRUE
        } else {
            Self::FALSE
        }
    }
    pub fn as_bool(self) -> bool {
        self.0 != 0
    }
    pub fn as_value(self) -> i32 {
        self.0
    }
}

impl From<bool> for IntBool {
    fn from(b: bool) -> Self {
        Self::new(b)
    }
}

impl std::fmt::Display for IntBool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.as_bool() { "yes" } else { "no" })
    }
}

/// Signed weight for coach-facing affinity tables (`pair_affinity`,
/// `rower_seat_affinity`). Semantics:
///
/// - Positive → prefer / want-together reward
/// - Negative → penalty / want-apart
/// - Zero is forbidden (meaningless; equivalent to no row). Enforced
///   both by the SQL CHECK and by the constructors below.
/// - Bounded ±5 to match the documented scale. Values outside that
///   range clamp to it.
///
/// Provides `as_int` for the solver's `.scaled(w)` call sites and
/// `is_affinity` / `is_anti` predicates for readable branches.
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
pub struct AffinityWeight(i32);

/// Lower inclusive bound on affinity weights.
pub const AFFINITY_WEIGHT_MIN: i32 = -5;
/// Upper inclusive bound on affinity weights.
pub const AFFINITY_WEIGHT_MAX: i32 = 5;

impl AffinityWeight {
    /// Construct from a raw integer, clamping to `[-5, 5] \ {0}`. A
    /// zero input is bumped to `+1` (the weakest positive affinity);
    /// non-zero inputs outside the range are clamped.
    pub fn new(n: i32) -> Self {
        let n = n.clamp(AFFINITY_WEIGHT_MIN, AFFINITY_WEIGHT_MAX);
        Self(if n == 0 { 1 } else { n })
    }

    /// Fallible constructor that rejects zero and out-of-range values
    /// rather than clamping. Prefer this for code paths where the
    /// caller should notice a bad value (e.g. admin CSV imports).
    pub fn try_new(n: i32) -> Option<Self> {
        if n == 0 || !(AFFINITY_WEIGHT_MIN..=AFFINITY_WEIGHT_MAX).contains(&n) {
            None
        } else {
            Some(Self(n))
        }
    }

    pub fn as_int(self) -> i32 {
        self.0
    }

    /// True for positive weights — the coach wants this pair / seat
    /// together.
    pub fn is_affinity(self) -> bool {
        self.0 > 0
    }

    /// True for negative weights — the coach wants this pair / seat
    /// avoided.
    pub fn is_anti(self) -> bool {
        self.0 < 0
    }
}

impl std::fmt::Display for AffinityWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 > 0 {
            write!(f, "+{}", self.0)
        } else {
            self.0.fmt(f)
        }
    }
}
