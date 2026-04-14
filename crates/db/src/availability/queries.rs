use super::types::AvailabilityStatus;
use super::{Availability, NewAvailability};
use crate::practice::PracticeId;
use crate::rower::types::RowerId;
use crate::schema::availability;
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
                availability::practice_id,
            ))
            .do_update()
            .set(availability::status.eq(new.status))
            .execute(conn)?;
        Ok(())
    }

    /// All availability records for a single practice.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_for_practice(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<Vec<Availability>, diesel::result::Error> {
        availability::table
            .filter(availability::practice_id.eq(practice_id))
            .select(Availability::as_select())
            .get_results(conn)
    }

    /// Indexed view keyed by rower id for a single practice.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn map_for_practice(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<HashMap<RowerId, AvailabilityStatus>, diesel::result::Error> {
        Ok(Self::list_for_practice(conn, practice_id)?
            .into_iter()
            .map(|a| (a.rower_id, a.status))
            .collect())
    }

    /// All availability records across multiple practices.
    /// Returns a map of (rower_id, practice_id) → status for grid lookups.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn map_for_practices(
        conn: &mut SqliteConnection,
        practice_ids: &[PracticeId],
    ) -> Result<HashMap<(RowerId, PracticeId), AvailabilityStatus>, diesel::result::Error> {
        let rows: Vec<Availability> = availability::table
            .filter(availability::practice_id.eq_any(practice_ids))
            .select(Availability::as_select())
            .get_results(conn)?;
        Ok(rows
            .into_iter()
            .map(|a| ((a.rower_id, a.practice_id), a.status))
            .collect())
    }

    /// Practice IDs that have any availability rows.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn practices_with_responses(
        conn: &mut SqliteConnection,
        practice_ids: &[PracticeId],
    ) -> Result<Vec<PracticeId>, diesel::result::Error> {
        availability::table
            .filter(availability::practice_id.eq_any(practice_ids))
            .select(availability::practice_id)
            .distinct()
            .get_results(conn)
    }
}
