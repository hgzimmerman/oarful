pub mod queries;
pub mod types;

use crate::practice::PracticeId;
use crate::rower::types::RowerId;
use types::AvailabilityStatus;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    diesel::Queryable,
    diesel::Selectable,
)]
#[diesel(table_name = crate::schema::availability)]
pub struct Availability {
    pub rower_id: RowerId,
    pub practice_id: PracticeId,
    pub status: AvailabilityStatus,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::availability)]
pub struct NewAvailability {
    pub rower_id: RowerId,
    pub practice_id: PracticeId,
    pub status: AvailabilityStatus,
}
