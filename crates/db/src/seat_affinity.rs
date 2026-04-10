//! A rower's affinity for a particular seat position. Positive weight
//! rewards the solver for placing the rower in that seat; negative weight
//! penalises it. `seat_position` is boat-agnostic — seat 4 means "seat 4
//! in any boat that has one", so a preference for a stroke-numbered seat
//! only applies to boats of that size.

use crate::rower::types::RowerId;
use crate::schema::rower_seat_affinity;
use crate::types::AffinityWeight;
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
    pub weight: AffinityWeight,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::rower_seat_affinity)]
pub struct NewSeatAffinity {
    pub rower_id: RowerId,
    pub seat_position: i32,
    pub weight: AffinityWeight,
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

    /// Every seat preference belonging to one rower, ordered by seat
    /// position. Used by the per-rower detail page.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn list_for_rower(
        conn: &mut SqliteConnection,
        rower: RowerId,
    ) -> Result<Vec<Self>, diesel::result::Error> {
        rower_seat_affinity::table
            .filter(rower_seat_affinity::rower_id.eq(rower))
            .order(rower_seat_affinity::seat_position.asc())
            .select(Self::as_select())
            .get_results(conn)
    }

    /// Insert or update one (rower, seat) preference. The unique key
    /// is `(rower_id, seat_position)` so the upsert collapses any
    /// existing row's weight to the new value.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn upsert(
        conn: &mut SqliteConnection,
        rower: RowerId,
        seat_position: i32,
        weight: AffinityWeight,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(rower_seat_affinity::table)
            .values(NewSeatAffinity {
                rower_id: rower,
                seat_position,
                weight,
            })
            .on_conflict((
                rower_seat_affinity::rower_id,
                rower_seat_affinity::seat_position,
            ))
            .do_update()
            .set(rower_seat_affinity::weight.eq(weight))
            .execute(conn)?;
        Ok(())
    }

    /// Remove one (rower, seat) preference. Silently no-ops if the
    /// row didn't exist.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn delete(
        conn: &mut SqliteConnection,
        rower: RowerId,
        seat_position: i32,
    ) -> Result<(), diesel::result::Error> {
        diesel::delete(
            rower_seat_affinity::table
                .filter(rower_seat_affinity::rower_id.eq(rower))
                .filter(rower_seat_affinity::seat_position.eq(seat_position)),
        )
        .execute(conn)?;
        Ok(())
    }
}
