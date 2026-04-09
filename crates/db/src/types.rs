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
