use serde::{Deserialize, Serialize};

/// A rower's commitment to a specific practice date, as synced from the
/// shared club spreadsheet.
///
/// - `Yes` — available for sweep seat assignment.
/// - `No` — not coming.
/// - `Maybe` — tentative; the solver treats them as unavailable by default
///   but the coach can promote them.
/// - `ScullingOnly` — attending as part of the scullers team that day. The
///   rower is still tracked in the system (attendance comes from the same
///   sheet) but the sweep solver excludes them from evaluation.
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
    Maybe,
    ScullingOnly,
}

impl AvailabilityStatus {
    /// Whether this status makes the rower a candidate for sweep seating today.
    /// `ScullingOnly` rowers are deliberately excluded — they're with the
    /// scullers team, not available for sweep assignment.
    pub fn is_available_for_sweep(self) -> bool {
        matches!(self, Self::Yes)
    }
}

impl std::fmt::Display for AvailabilityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Yes => "Yes",
            Self::No => "No",
            Self::Maybe => "Maybe",
            Self::ScullingOnly => "ScullingOnly",
        })
    }
}
