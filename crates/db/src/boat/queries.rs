use super::types::BoatId;
use super::{Boat, NewBoat};
use crate::schema::{boat, lineup, practice};
use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::SqliteConnection;

/// Usage statistics derived from committed lineups for past practices.
#[derive(Debug, Clone)]
pub struct BoatUsageSummary {
    pub total_uses: i64,
    pub last_used: Option<NaiveDate>,
    /// Distinct practice (id, date) pairs this boat was used, most recent first.
    pub recent_uses: Vec<(crate::practice::PracticeId, NaiveDate)>,
}

impl Boat {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn insert(
        conn: &mut SqliteConnection,
        new: NewBoat,
    ) -> Result<Boat, diesel::result::Error> {
        diesel::insert_into(boat::table)
            .values(new)
            .returning(Boat::as_returning())
            .get_result(conn)
    }

    /// Every boat, in-service first then relinquished, ordered by seat
    /// count descending within each group. Used by the admin list view.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_all(conn: &mut SqliteConnection) -> Result<Vec<Boat>, diesel::result::Error> {
        boat::table
            .select(Boat::as_select())
            .order((
                boat::relinquished_at.asc(),
                boat::seat_count.desc(),
                boat::name.asc(),
            ))
            .get_results(conn)
    }

    /// All in-service boats (not yet relinquished).
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_in_service(
        conn: &mut SqliteConnection,
    ) -> Result<Vec<Boat>, diesel::result::Error> {
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

    /// Look up a boat by id. Returns `Ok(None)` for an unknown id.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn get(
        conn: &mut SqliteConnection,
        id: BoatId,
    ) -> Result<Option<Boat>, diesel::result::Error> {
        boat::table
            .find(id)
            .select(Boat::as_select())
            .first(conn)
            .optional()
    }

    /// Usage summary for a single boat, derived from committed lineups
    /// where the practice date is in the past.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn usage_summary(
        conn: &mut SqliteConnection,
        id: BoatId,
    ) -> Result<BoatUsageSummary, diesel::result::Error> {
        let today = chrono::Local::now().date_naive();

        let recent_uses: Vec<(crate::practice::PracticeId, NaiveDate)> = lineup::table
            .inner_join(practice::table)
            .filter(lineup::boat_id.eq(id))
            .filter(practice::date.lt(today))
            .select((practice::id, practice::date))
            .distinct()
            .order(practice::date.desc())
            .get_results(conn)?;

        let total_uses = recent_uses.len() as i64;
        let last_used = recent_uses.first().map(|(_, d)| *d);

        Ok(BoatUsageSummary {
            total_uses,
            last_used,
            recent_uses,
        })
    }

    /// Persist all fields of `boat` back to its row. Mirrors
    /// [`crate::rower::Rower::save`] — load via `get`, mutate, save.
    #[tracing::instrument(level = "debug", skip(conn, boat), err)]
    pub fn save(conn: &mut SqliteConnection, boat: &Boat) -> Result<Boat, diesel::result::Error> {
        diesel::update(boat::table.filter(boat::id.eq(boat.id)))
            .set(boat)
            .returning(Boat::as_returning())
            .get_result(conn)
    }
}
