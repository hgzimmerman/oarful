use serde::{Deserialize, Serialize};

/// A rower's commitment to a specific practice date.
///
/// - `Yes` — available for seat assignment.
/// - `No` — not coming.
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
pub enum AvailabilityStatus {
    Yes,
    No,
}

impl AvailabilityStatus {
    /// Whether this status makes the rower a candidate for seating today.
    pub fn is_available(self) -> bool {
        matches!(self, Self::Yes)
    }
}

impl std::fmt::Display for AvailabilityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Yes => "Yes",
            Self::No => "No",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_yes_is_available() {
        assert!(AvailabilityStatus::Yes.is_available());
    }

    #[test]
    fn availability_no_is_not_available() {
        assert!(!AvailabilityStatus::No.is_available());
    }

    #[test]
    fn availability_display() {
        assert_eq!(AvailabilityStatus::Yes.to_string(), "Yes");
        assert_eq!(AvailabilityStatus::No.to_string(), "No");
    }
}
