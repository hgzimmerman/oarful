//! A rower's affinity for a particular seat position. Positive weight
//! rewards the solver for placing the rower in that seat; negative weight
//! penalises it. `seat_position` is boat-agnostic — seat 4 means "seat 4
//! in any boat that has one", so a preference for a stroke-numbered seat
//! only applies to boats of that size.

use crate::rower::types::RowerId;
use crate::schema::rower_seat_affinity;
use diesel::prelude::*;
use diesel::SqliteConnection;

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
#[diesel(table_name = crate::schema::rower_seat_affinity)]
pub struct SeatAffinity {
    pub rower_id: RowerId,
    pub seat_position: i32,
    pub weight: i32,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::rower_seat_affinity)]
pub struct NewSeatAffinity {
    pub rower_id: RowerId,
    pub seat_position: i32,
    pub weight: i32,
}

impl SeatAffinity {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn insert(
        conn: &mut SqliteConnection,
        new: NewSeatAffinity,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(rower_seat_affinity::table)
            .values(new)
            .execute(conn)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_all(conn: &mut SqliteConnection) -> Result<Vec<Self>, diesel::result::Error> {
        rower_seat_affinity::table
            .select(Self::as_select())
            .get_results(conn)
    }
}
