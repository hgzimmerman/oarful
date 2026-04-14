use super::types::AvailabilityStatus;
use super::{Availability, NewAvailability};
use crate::rower::types::RowerId;
use crate::schema::availability;
use crate::team::TeamId;
use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::SqliteConnection;
use std::collections::HashMap;

impl Availability {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn upsert(
        conn: &mut SqliteConnection,
        new: NewAvailability,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(availability::table)
            .values(&new)
            .on_conflict((
                availability::rower_id,
                availability::team_id,
                availability::date,
            ))
            .do_update()
            .set(availability::status.eq(new.status))
            .execute(conn)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_for_team_date(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        date: NaiveDate,
    ) -> Result<Vec<Availability>, diesel::result::Error> {
        availability::table
            .filter(availability::team_id.eq(team_id))
            .filter(availability::date.eq(date))
            .select(Availability::as_select())
            .get_results(conn)
    }

    /// Indexed view keyed by rower id for snapshot joins. Scoped to
    /// one (team, date).
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn map_for_team_date(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        date: NaiveDate,
    ) -> Result<HashMap<RowerId, AvailabilityStatus>, diesel::result::Error> {
        Ok(Self::list_for_team_date(conn, team_id, date)?
            .into_iter()
            .map(|a| (a.rower_id, a.status))
            .collect())
    }

    /// All availability records for a team across a range of dates.
    /// Returns a map of (rower_id, date) → status for efficient grid lookups.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn map_for_team_dates(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        dates: &[NaiveDate],
    ) -> Result<HashMap<(RowerId, NaiveDate), AvailabilityStatus>, diesel::result::Error> {
        let rows: Vec<Availability> = availability::table
            .filter(availability::team_id.eq(team_id))
            .filter(availability::date.eq_any(dates))
            .select(Availability::as_select())
            .get_results(conn)?;
        Ok(rows
            .into_iter()
            .map(|a| ((a.rower_id, a.date), a.status))
            .collect())
    }

    /// Distinct practice dates on or after `today` that have any
    /// availability rows for the given team. Chronological order.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn upcoming_dates(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        today: NaiveDate,
    ) -> Result<Vec<NaiveDate>, diesel::result::Error> {
        availability::table
            .filter(availability::team_id.eq(team_id))
            .filter(availability::date.ge(today))
            .select(availability::date)
            .distinct()
            .order(availability::date.asc())
            .get_results(conn)
    }
}
