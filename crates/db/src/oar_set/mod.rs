pub mod queries;
pub mod types;

pub use types::{OarSetId, OarType};

use crate::boat::types::BoatId;
use crate::practice::PracticeId;
use crate::types::IntBool;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Identifiable,
    diesel::AsChangeset,
)]
#[diesel(table_name = crate::schema::oar_set)]
pub struct OarSet {
    pub id: OarSetId,
    pub name: String,
    pub oar_count: i32,
    pub notes: Option<String>,
    pub active: IntBool,
    pub created_at: chrono::NaiveDateTime,
    pub oar_type: OarType,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::oar_set)]
pub struct NewOarSet {
    pub name: String,
    pub oar_count: i32,
    pub notes: Option<String>,
    pub oar_type: OarType,
}

#[derive(
    Debug, Clone, PartialEq, Eq, diesel::Queryable, diesel::Selectable, diesel::Identifiable,
)]
#[diesel(table_name = crate::schema::oar_set_preference)]
pub struct OarSetPreference {
    pub id: i32,
    pub oar_set_id: OarSetId,
    pub boat_id: BoatId,
    pub priority: i32,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::oar_set_preference)]
pub struct NewOarSetPreference {
    pub oar_set_id: OarSetId,
    pub boat_id: BoatId,
    pub priority: i32,
}

#[derive(
    Debug, Clone, PartialEq, Eq, diesel::Queryable, diesel::Selectable, diesel::Identifiable,
)]
#[diesel(table_name = crate::schema::practice_boat_oars)]
pub struct PracticeBoatOars {
    pub id: i32,
    pub practice_id: PracticeId,
    pub boat_id: BoatId,
    pub oar_set_id: OarSetId,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::practice_boat_oars)]
pub struct NewPracticeBoatOars {
    pub practice_id: PracticeId,
    pub boat_id: BoatId,
    pub oar_set_id: OarSetId,
}
