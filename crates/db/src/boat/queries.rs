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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boat::types::{CoxPosition, OarsPerSeat, SeatCount, WeightClass};
    use crate::boat::NewBoat;
    use crate::rower::types::Side;
    use crate::test_support::in_memory_conn;
    use crate::types::IntBool;

    fn make_boat(name: &str, oars: i32) -> NewBoat {
        NewBoat {
            name: name.into(),
            weight_class: WeightClass::Heavy,
            seat_count: SeatCount::new(8),
            has_cox: IntBool::TRUE,
            oars_per_seat: OarsPerSeat::new(oars),
            acquired_at: None,
            manufactured_at: None,
            stroke_side: Side::Starboard,
            cox_position: CoxPosition::Stern,
        }
    }

    #[test]
    fn insert_and_get() {
        let mut conn = in_memory_conn();
        let b = Boat::insert(&mut conn, make_boat("Spirit", 1)).unwrap();
        let fetched = Boat::get(&mut conn, b.id).unwrap().unwrap();
        assert_eq!(fetched.name, "Spirit");
        assert!(Boat::get(&mut conn, BoatId::new(9999)).unwrap().is_none());
    }

    #[test]
    fn list_sweep_excludes_sculls() {
        let mut conn = in_memory_conn();
        Boat::insert(&mut conn, make_boat("Sweep 8+", 1)).unwrap();
        Boat::insert(&mut conn, make_boat("Quad", 2)).unwrap();

        let sweep = Boat::list_sweep(&mut conn).unwrap();
        assert_eq!(sweep.len(), 1);
        assert_eq!(sweep[0].name, "Sweep 8+");
    }

    #[test]
    fn list_in_service_excludes_relinquished() {
        let mut conn = in_memory_conn();
        Boat::insert(&mut conn, make_boat("Active", 1)).unwrap();
        let mut retired = make_boat("Retired", 1);
        retired.name = "Retired".into();
        let b = Boat::insert(&mut conn, retired).unwrap();
        let mut b = b;
        b.relinquished_at = Some(chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
        Boat::save(&mut conn, &b).unwrap();

        let in_service = Boat::list_in_service(&mut conn).unwrap();
        assert_eq!(in_service.len(), 1);
        assert_eq!(in_service[0].name, "Active");
    }

    #[test]
    fn is_sweep_and_is_scull() {
        let mut conn = in_memory_conn();
        let sweep = Boat::insert(&mut conn, make_boat("S", 1)).unwrap();
        let scull = Boat::insert(&mut conn, make_boat("Q", 2)).unwrap();

        assert!(sweep.is_sweep());
        assert!(!sweep.is_scull());
        assert!(!scull.is_sweep());
        assert!(scull.is_scull());
    }

    #[test]
    fn seat_side_alternating_rig() {
        let mut conn = in_memory_conn();
        let b = Boat::insert(&mut conn, make_boat("Eight", 1)).unwrap();
        // stroke_side = Starboard, seat 8 = stroke
        assert_eq!(b.seat_side(8), Some(Side::Starboard)); // stroke
        assert_eq!(b.seat_side(7), Some(Side::Port)); // one from stroke
        assert_eq!(b.seat_side(1), Some(Side::Port)); // bow (7 from stroke, odd)
    }

    #[test]
    fn seat_side_returns_none_for_cox_and_scull() {
        let mut conn = in_memory_conn();
        let sweep = Boat::insert(&mut conn, make_boat("S", 1)).unwrap();
        assert!(sweep.seat_side(0).is_none()); // cox seat

        let scull = Boat::insert(&mut conn, make_boat("Q", 2)).unwrap();
        assert!(scull.seat_side(1).is_none()); // scull has no side
    }

    #[test]
    fn save_updates() {
        let mut conn = in_memory_conn();
        let mut b = Boat::insert(&mut conn, make_boat("Old Name", 1)).unwrap();
        b.name = "New Name".into();
        let saved = Boat::save(&mut conn, &b).unwrap();
        assert_eq!(saved.name, "New Name");
    }
}
