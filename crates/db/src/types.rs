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

/// A duration expressed in minutes. Used for practice durations and
/// sync poll intervals.
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
pub struct DurationMinutes(i32);

impl DurationMinutes {
    pub fn new(n: i32) -> Self {
        Self(n)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for DurationMinutes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for DurationMinutes {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

/// Body weight in kilograms.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct WeightKg(f64);

impl WeightKg {
    pub fn new(kg: f64) -> Self {
        Self(kg)
    }
    pub fn as_f64(self) -> f64 {
        self.0
    }
    pub fn to_lbs(self) -> f64 {
        self.0 * 2.20462
    }
}

impl std::fmt::Display for WeightKg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}", self.0)
    }
}

/// Height in metres.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct HeightM(f64);

impl HeightM {
    pub fn new(m: f64) -> Self {
        Self(m)
    }
    pub fn as_f64(self) -> f64 {
        self.0
    }
    pub fn to_inches(self) -> f64 {
        self.0 * 39.3701
    }
    pub fn to_ft_in(self) -> String {
        let total_inches = self.to_inches();
        let feet = total_inches as i32 / 12;
        let inches = (total_inches.round() as i32) % 12;
        format!("{feet}'{inches}\"")
    }
}

impl std::fmt::Display for HeightM {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

// ── String newtypes ──────────────────────────────────────────────

macro_rules! string_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
            Serialize, Deserialize,
            diesel_derive_newtype::DieselNewType,
        )]
        pub struct $name(String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_newtype!(
    /// What happened: "boat.create", "rower.update", "lineup.commit", etc.
    AuditAction
);
string_newtype!(
    /// Which entity kind was affected: "boat", "rower", "practice", etc.
    AuditResourceType
);
string_newtype!(
    /// Which specific entity (typically the stringified primary key).
    AuditResourceId
);
string_newtype!(
    /// Email blast type stored in the rate-limiting log: "reminder", "lineup".
    EmailLogType
);
string_newtype!(
    /// Sync integration kind: "google_sheet".
    SyncSourceType
);

// ── Numeric newtypes ─────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    use super::*;

    // ── IntBool ──────────────────────────────────────────────────────

    #[test]
    fn int_bool_round_trip() {
        assert!(IntBool::new(true).as_bool());
        assert!(!IntBool::new(false).as_bool());
        assert_eq!(IntBool::TRUE.as_value(), 1);
        assert_eq!(IntBool::FALSE.as_value(), 0);
    }

    #[test]
    fn int_bool_from_bool() {
        let t: IntBool = true.into();
        let f: IntBool = false.into();
        assert!(t.as_bool());
        assert!(!f.as_bool());
    }

    #[test]
    fn int_bool_nonzero_is_true() {
        // Any non-zero value should be truthy
        assert!(IntBool(42).as_bool());
        assert!(IntBool(-1).as_bool());
    }

    #[test]
    fn int_bool_display() {
        assert_eq!(IntBool::TRUE.to_string(), "yes");
        assert_eq!(IntBool::FALSE.to_string(), "no");
    }

    // ── DurationMinutes ─────────────────────────────────────────────

    #[test]
    fn duration_minutes_round_trip() {
        let d = DurationMinutes::new(90);
        assert_eq!(d.as_int(), 90);
        assert_eq!(d.to_string(), "90");
    }

    #[test]
    fn duration_minutes_from_str() {
        let d: DurationMinutes = "120".parse().unwrap();
        assert_eq!(d.as_int(), 120);
        assert!("abc".parse::<DurationMinutes>().is_err());
    }

    // ── WeightKg ────────────────────────────────────────────────────

    #[test]
    fn weight_kg_to_lbs() {
        let w = WeightKg::new(100.0);
        let lbs = w.to_lbs();
        assert!((lbs - 220.462).abs() < 0.01);
    }

    #[test]
    fn weight_kg_display() {
        assert_eq!(WeightKg::new(80.0).to_string(), "80.0");
        assert_eq!(WeightKg::new(72.56).to_string(), "72.6"); // rounds to 1 decimal
    }

    // ── HeightM ─────────────────────────────────────────────────────

    #[test]
    fn height_m_to_inches() {
        let h = HeightM::new(1.0);
        assert!((h.to_inches() - 39.3701).abs() < 0.01);
    }

    #[test]
    fn height_m_to_ft_in_known_heights() {
        // 6'0" = 1.8288m
        assert_eq!(HeightM::new(1.8288).to_ft_in(), "6'0\"");
        // 5'11" ≈ 1.8034m
        assert_eq!(HeightM::new(1.8034).to_ft_in(), "5'11\"");
    }

    #[test]
    fn height_m_display() {
        assert_eq!(HeightM::new(1.83).to_string(), "1.83");
    }

    // ── String newtypes (macro-generated) ───────────────────────────

    #[test]
    fn string_newtype_round_trip() {
        let a = AuditAction::new("boat.create");
        assert_eq!(a.as_str(), "boat.create");
        assert_eq!(a.to_string(), "boat.create");

        let e = EmailLogType::new("reminder");
        assert_eq!(e.as_str(), "reminder");

        let s = SyncSourceType::new("google_sheet");
        assert_eq!(s.as_str(), "google_sheet");
    }

    #[test]
    fn string_newtype_from_owned_string() {
        let a = AuditAction::new(String::from("rower.update"));
        assert_eq!(a.as_str(), "rower.update");
    }

    // ── AffinityWeight ──────────────────────────────────────────────

    #[test]
    fn affinity_weight_clamps_to_range() {
        assert_eq!(AffinityWeight::new(10).as_int(), 5);
        assert_eq!(AffinityWeight::new(-10).as_int(), -5);
    }

    #[test]
    fn affinity_weight_zero_bumps_to_one() {
        assert_eq!(AffinityWeight::new(0).as_int(), 1);
    }

    #[test]
    fn affinity_weight_in_range_unchanged() {
        for n in [-5, -3, -1, 1, 3, 5] {
            assert_eq!(AffinityWeight::new(n).as_int(), n);
        }
    }

    #[test]
    fn affinity_weight_try_new_rejects_zero() {
        assert!(AffinityWeight::try_new(0).is_none());
    }

    #[test]
    fn affinity_weight_try_new_rejects_out_of_range() {
        assert!(AffinityWeight::try_new(6).is_none());
        assert!(AffinityWeight::try_new(-6).is_none());
    }

    #[test]
    fn affinity_weight_try_new_accepts_valid() {
        for n in [-5, -1, 1, 5] {
            assert_eq!(AffinityWeight::try_new(n).unwrap().as_int(), n);
        }
    }

    #[test]
    fn affinity_weight_predicates() {
        assert!(AffinityWeight::new(3).is_affinity());
        assert!(!AffinityWeight::new(3).is_anti());
        assert!(AffinityWeight::new(-3).is_anti());
        assert!(!AffinityWeight::new(-3).is_affinity());
    }

    #[test]
    fn affinity_weight_display() {
        assert_eq!(AffinityWeight::new(3).to_string(), "+3");
        assert_eq!(AffinityWeight::new(-2).to_string(), "-2");
    }
}
