use super::{Boat, NewBoat};
use crate::schema::boat;
use diesel::prelude::*;
use diesel::SqliteConnection;

impl Boat {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn insert(conn: &mut SqliteConnection, new: NewBoat) -> Result<Boat, diesel::result::Error> {
        diesel::insert_into(boat::table)
            .values(new)
            .returning(Boat::as_returning())
            .get_result(conn)
    }

    /// All in-service boats (not yet relinquished).
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_in_service(conn: &mut SqliteConnection) -> Result<Vec<Boat>, diesel::result::Error> {
        boat::table
            .filter(boat::relinquished_at.is_null())
            .select(Boat::as_select())
            .order(boat::seat_count.desc())
            .get_results(conn)
    }

    /// All in-service SWEEP boats — the only candidates for this solver.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_sweep(conn: &mut SqliteConnection) -> Result<Vec<Boat>, diesel::result::Error> {
        boat::table
            .filter(boat::relinquished_at.is_null())
            .filter(boat::oars_per_seat.eq(1))
            .select(Boat::as_select())
            .order(boat::seat_count.desc())
            .get_results(conn)
    }
}
