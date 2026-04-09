use super::types::AvailabilityStatus;
use super::{Availability, NewAvailability};
use crate::rower::types::RowerId;
use crate::schema::availability;
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
        // sqlite UPSERT via `ON CONFLICT ... DO UPDATE`.
        diesel::insert_into(availability::table)
            .values(&new)
            .on_conflict((availability::rower_id, availability::date))
            .do_update()
            .set(availability::status.eq(new.status))
            .execute(conn)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_for_date(
        conn: &mut SqliteConnection,
        date: NaiveDate,
    ) -> Result<Vec<Availability>, diesel::result::Error> {
        availability::table
            .filter(availability::date.eq(date))
            .select(Availability::as_select())
            .get_results(conn)
    }

    /// Indexed view of the above, keyed by rower id for snapshot joins.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn map_for_date(
        conn: &mut SqliteConnection,
        date: NaiveDate,
    ) -> Result<HashMap<RowerId, AvailabilityStatus>, diesel::result::Error> {
        Ok(Self::list_for_date(conn, date)?
            .into_iter()
            .map(|a| (a.rower_id, a.status))
            .collect())
    }
}
