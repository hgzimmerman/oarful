use super::types::{RowerId, Side};
use super::{NewRower, Rower};
use crate::schema::{lineup, lineup_seat, practice, rower};
use crate::types::IntBool;
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

    /// Look up a rower by email. Used as the matching key during the
    /// Google Sheets sync. Returns `Ok(None)` if no rower with that
    /// email exists.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn find_by_email(
        conn: &mut SqliteConnection,
        email: &str,
    ) -> Result<Option<Rower>, diesel::result::Error> {
        rower::table
            .filter(rower::email.eq(email))
            .select(Rower::as_select())
            .first(conn)
            .optional()
    }

    /// Promote-only update of mutable rower attributes from sheet data.
    ///
    /// Rule: the sync path NEVER demotes a coach-set value. Specific
    /// sides stay specific, true flags stay true. The sync only
    /// promotes:
    /// - `side`: `Either` → `Port` or `Starboard`
    /// - `can_cox`: `false` → `true`
    /// - `can_scull`: `false` → `true`
    /// - `is_designated_cox`: `false` → `true`
    ///
    /// Also updates `name` unconditionally (the sheet is the authoritative
    /// display name) and `updated_at` to now.
    ///
    /// Returns the updated Rower. If no fields changed, the existing
    /// row is returned unchanged and no UPDATE is issued.
    #[tracing::instrument(level = "debug", skip(conn, current), err)]
    pub fn promote_from_sheet(
        conn: &mut SqliteConnection,
        current: &Rower,
        new_name: &str,
        new_side: Side,
        new_can_scull: bool,
        new_can_cox: bool,
        new_is_designated_cox: bool,
    ) -> Result<Rower, diesel::result::Error> {
        let mut dirty = false;
        let mut next = current.clone();

        if next.name != new_name {
            next.name = new_name.to_string();
            dirty = true;
        }
        // Side: promote Either → specific; never demote.
        if next.side == Side::Either && new_side != Side::Either {
            next.side = new_side;
            dirty = true;
        }
        // Boolean flags: promote false → true; never demote.
        if !next.can_scull.as_bool() && new_can_scull {
            next.can_scull = IntBool::TRUE;
            dirty = true;
        }
        if !next.can_cox.as_bool() && new_can_cox {
            next.can_cox = IntBool::TRUE;
            dirty = true;
        }
        if !next.is_designated_cox.as_bool() && new_is_designated_cox {
            next.is_designated_cox = IntBool::TRUE;
            dirty = true;
        }

        if !dirty {
            return Ok(current.clone());
        }

        next.updated_at = chrono::Utc::now().naive_utc();
        diesel::update(rower::table.filter(rower::id.eq(current.id)))
            .set(&next)
            .returning(Rower::as_returning())
            .get_result(conn)
    }
}
