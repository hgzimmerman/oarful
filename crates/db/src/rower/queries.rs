use super::types::RowerId;
use super::{NewRower, Rower};
use crate::schema::{lineup, lineup_seat, practice, rower};
use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::SqliteConnection;
use std::collections::HashMap;

impl Rower {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn insert(
        conn: &mut SqliteConnection,
        new: NewRower,
    ) -> Result<Rower, diesel::result::Error> {
        diesel::insert_into(rower::table)
            .values(new)
            .returning(Rower::as_returning())
            .get_result(conn)
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_active(conn: &mut SqliteConnection) -> Result<Vec<Rower>, diesel::result::Error> {
        rower::table
            .filter(rower::active.eq(1))
            .select(Rower::as_select())
            .order(rower::name.asc())
            .get_results(conn)
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn count(conn: &mut SqliteConnection) -> Result<i64, diesel::result::Error> {
        rower::table.count().get_result(conn)
    }

    /// When each rower last coxed, derived from the `lineup_seat` history.
    /// Rowers who have never coxed are absent from the returned map.
    ///
    /// Replaces a denormalised `rower.last_coxed_on` column — the lineup
    /// history is the source of truth.
    ///
    /// We load every cox appearance and reduce in Rust rather than asking
    /// diesel to emit a `GROUP BY rower_id, MAX(date)` query; the latter is
    /// finicky to type across a three-table join in diesel 2.x and the
    /// dataset is tiny (bounded by cox appearances ever).
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn last_coxed_dates(
        conn: &mut SqliteConnection,
    ) -> Result<HashMap<RowerId, NaiveDate>, diesel::result::Error> {
        let rows: Vec<(RowerId, NaiveDate)> = lineup_seat::table
            .inner_join(lineup::table.on(lineup::id.eq(lineup_seat::lineup_id)))
            .inner_join(practice::table.on(practice::id.eq(lineup::practice_id)))
            .filter(lineup_seat::is_cox.eq(1))
            .select((lineup_seat::rower_id, practice::date))
            .load(conn)?;

        let mut map: HashMap<RowerId, NaiveDate> = HashMap::new();
        for (rid, date) in rows {
            map.entry(rid)
                .and_modify(|existing| {
                    if date > *existing {
                        *existing = date;
                    }
                })
                .or_insert(date);
        }
        Ok(map)
    }
}
