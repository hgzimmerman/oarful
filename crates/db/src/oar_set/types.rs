use serde::{Deserialize, Serialize};

/// Whether an oar set is for sweep rowing (1 oar per rower) or
/// sculling (2 oars per rower). Sweep oars and sculling oars are
/// physically different and never interchangeable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, diesel_derive_enum::DbEnum)]
#[DbValueStyle = "verbatim"]
pub enum OarType {
    #[serde(rename = "sweep")]
    #[db_rename = "sweep"]
    Sweep,
    #[serde(rename = "sculling")]
    #[db_rename = "sculling"]
    Sculling,
}

impl std::fmt::Display for OarType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OarType::Sweep => "Sweep",
            OarType::Sculling => "Sculling",
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
    Hash,
    Serialize,
    Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct OarSetId(i32);

impl OarSetId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for OarSetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for OarSetId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}
