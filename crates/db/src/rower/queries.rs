use super::types::{RowerId, Side};
use super::{NewRower, Rower};
use crate::schema::{availability, lineup, lineup_seat, practice, rower};
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

    /// Look up a rower by id. Returns `Ok(None)` for an unknown id —
    /// callers convert that into whatever they need (typically a 404).
    /// Other database errors propagate as `Err`.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn get(
        conn: &mut SqliteConnection,
        id: RowerId,
    ) -> Result<Option<Rower>, diesel::result::Error> {
        rower::table
            .find(id)
            .select(Rower::as_select())
            .first(conn)
            .optional()
    }

    /// Persist all fields of `rower` to the matching row, bumping
    /// `updated_at` to now. Used by the admin UI's inline-edit flow:
    /// load via [`Rower::get`], mutate the editable fields, then call
    /// `save`. Returns the freshly-loaded row so callers can render
    /// the canonical state without an extra round trip.
    ///
    /// Distinct from [`Rower::promote_from_sheet`], which is the
    /// promote-only sync path. `save` is the unrestricted path:
    /// caller-supplied values land verbatim. Validate at the call site.
    #[tracing::instrument(level = "debug", skip(conn, rower), err)]
    pub fn save(
        conn: &mut SqliteConnection,
        rower: &Rower,
    ) -> Result<Rower, diesel::result::Error> {
        let mut next = rower.clone();
        next.updated_at = chrono::Utc::now().naive_utc();
        diesel::update(rower::table.filter(rower::id.eq(rower.id)))
            .set(&next)
            .returning(Rower::as_returning())
            .get_result(conn)
    }

    /// Find the rower linked to a user account. Returns None if the
    /// user doesn't have a linked rower (e.g. a PD who doesn't row).
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn find_by_user_id(
        conn: &mut SqliteConnection,
        uid: i32,
    ) -> Result<Option<Rower>, diesel::result::Error> {
        rower::table
            .filter(rower::user_id.eq(uid))
            .select(Rower::as_select())
            .first(conn)
            .optional()
    }

    /// Link a rower to a user account.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn link_to_user(
        conn: &mut SqliteConnection,
        rower_id: RowerId,
        uid: i32,
    ) -> Result<(), diesel::result::Error> {
        diesel::update(rower::table.filter(rower::id.eq(rower_id)))
            .set(rower::user_id.eq(uid))
            .execute(conn)?;
        Ok(())
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

    /// For each rower, find the most recent committed practice date
    /// where they were available (status = Yes) but not placed in any
    /// lineup seat. "Benched" = available AND not in lineup_seat.
    ///
    /// Strategy: load all (rower_id, date) pairs where the rower had
    /// availability = Yes for a practice that has at least one committed
    /// lineup, then subtract rowers who appear in a lineup_seat for
    /// that practice. Reduce per-rower to the most recent such date.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn last_benched_dates(
        conn: &mut SqliteConnection,
    ) -> Result<HashMap<RowerId, NaiveDate>, diesel::result::Error> {
        use crate::availability::types::AvailabilityStatus;
        use std::collections::HashSet;

        // Step 1: all (rower, date) pairs where the rower was available
        // for a committed practice (practice has at least one lineup).
        let committed_practice_ids: Vec<i32> = practice::table
            .filter(practice::id.eq_any(lineup::table.select(lineup::practice_id)))
            .select(practice::id)
            .get_results(conn)?;

        if committed_practice_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Load the practice dates for committed practices.
        let committed_dates: Vec<(i32, NaiveDate)> = practice::table
            .filter(practice::id.eq_any(&committed_practice_ids))
            .select((practice::id, practice::date))
            .get_results(conn)?;
        let practice_id_to_date: HashMap<i32, NaiveDate> = committed_dates
            .iter()
            .map(|(id, d)| (*id, *d))
            .collect();
        let dates: Vec<NaiveDate> = committed_dates.iter().map(|(_, d)| *d).collect();

        // All available rowers per committed date.
        let avail_rows: Vec<(RowerId, NaiveDate, AvailabilityStatus)> = availability::table
            .filter(availability::date.eq_any(&dates))
            .select((availability::rower_id, availability::date, availability::status))
            .get_results(conn)?;

        let mut available_by_date: HashMap<NaiveDate, HashSet<RowerId>> = HashMap::new();
        for (rid, date, status) in &avail_rows {
            if status.is_available_for_sweep() {
                available_by_date.entry(*date).or_default().insert(*rid);
            }
        }

        // Step 2: all placed rowers per committed practice.
        let placed_rows: Vec<(RowerId, i32)> = lineup_seat::table
            .inner_join(lineup::table.on(lineup::id.eq(lineup_seat::lineup_id)))
            .filter(lineup::practice_id.eq_any(&committed_practice_ids))
            .select((lineup_seat::rower_id, lineup::practice_id))
            .get_results(conn)?;

        let mut placed_by_date: HashMap<NaiveDate, HashSet<RowerId>> = HashMap::new();
        for (rid, pid) in &placed_rows {
            if let Some(&date) = practice_id_to_date.get(pid) {
                placed_by_date.entry(date).or_default().insert(*rid);
            }
        }

        // Step 3: benched = available - placed, per date. Keep most recent.
        let mut map: HashMap<RowerId, NaiveDate> = HashMap::new();
        for (date, available) in &available_by_date {
            let placed = placed_by_date.get(date);
            for rid in available {
                let was_placed = placed.map(|s| s.contains(rid)).unwrap_or(false);
                if !was_placed {
                    map.entry(*rid)
                        .and_modify(|existing| {
                            if *date > *existing {
                                *existing = *date;
                            }
                        })
                        .or_insert(*date);
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rower::types::{Height, RowerWeightClass, SideStrength, Skill, Strength};
    use crate::rower::NewRower;
    use crate::test_support::in_memory_conn;
    use crate::types::IntBool;

    /// Seed a single rower with the given side and flag state. Used
    /// as the "existing row" in promote_from_sheet tests.
    fn seed_rower(
        conn: &mut SqliteConnection,
        email: &str,
        side: Side,
        can_scull: bool,
        can_cox: bool,
        is_designated_cox: bool,
    ) -> Rower {
        let now = chrono::Utc::now().naive_utc();
        Rower::insert(
            conn,
            NewRower {
                name: "Seed Rower".into(),
                email: Some(email.into()),
                weight_class: RowerWeightClass::Medium,
                skill: Skill::Intermediate,
                strength: Strength::Intermediate,
                height: Height::Medium,
                side,
                side_strength: SideStrength::default(),
                can_scull: IntBool::new(can_scull),
                can_cox: IntBool::new(can_cox),
                is_designated_cox: IntBool::new(is_designated_cox),
                active: IntBool::TRUE,
                created_at: now,
                updated_at: now,
            },
        )
        .expect("seed rower insert")
    }

    #[test]
    fn find_by_email_returns_none_for_unknown() {
        let mut conn = in_memory_conn();
        assert!(Rower::find_by_email(&mut conn, "nobody@example.com")
            .unwrap()
            .is_none());
    }

    #[test]
    fn find_by_email_returns_existing_rower() {
        let mut conn = in_memory_conn();
        let seeded = seed_rower(
            &mut conn,
            "alice@example.com",
            Side::Port,
            false,
            true,
            false,
        );
        let found = Rower::find_by_email(&mut conn, "alice@example.com")
            .unwrap()
            .expect("should find seeded rower");
        assert_eq!(found.id, seeded.id);
        assert_eq!(found.side, Side::Port);
    }

    #[test]
    fn promote_from_sheet_never_demotes_specific_side_to_either() {
        // Core load-bearing rule: a coach's specific side assignment
        // (Port / Starboard) must survive a sync that carries Side::Either.
        let mut conn = in_memory_conn();
        let seeded = seed_rower(
            &mut conn,
            "alice@example.com",
            Side::Starboard,
            false,
            true,
            false,
        );

        let updated = Rower::promote_from_sheet(
            &mut conn,
            &seeded,
            "Alice Smith",
            Side::Either, // sheet says Both / unknown
            false,
            true,
            false,
        )
        .unwrap();

        assert_eq!(updated.side, Side::Starboard);
        // Name still updates — that's unconditional per the function's
        // documented contract, distinct from the side rule.
        assert_eq!(updated.name, "Alice Smith");
    }

    #[test]
    fn promote_from_sheet_does_promote_either_to_specific() {
        let mut conn = in_memory_conn();
        let seeded = seed_rower(
            &mut conn,
            "alice@example.com",
            Side::Either,
            false,
            true,
            false,
        );

        let updated = Rower::promote_from_sheet(
            &mut conn,
            &seeded,
            "Alice Smith",
            Side::Port,
            false,
            true,
            false,
        )
        .unwrap();

        assert_eq!(updated.side, Side::Port);
    }

    #[test]
    fn promote_from_sheet_never_demotes_true_flags_to_false() {
        // can_scull / can_cox / is_designated_cox should only ever go
        // false → true, never the reverse.
        let mut conn = in_memory_conn();
        let seeded = seed_rower(
            &mut conn,
            "alice@example.com",
            Side::Port,
            true,  // can_scull
            true,  // can_cox
            true,  // is_designated_cox
        );

        let updated = Rower::promote_from_sheet(
            &mut conn,
            &seeded,
            "Alice Smith",
            Side::Port,
            false, // sheet says she can't scull — ignored
            false, // sheet says she can't cox — ignored
            false, // sheet says she's not a designated cox — ignored
        )
        .unwrap();

        assert_eq!(updated.can_scull, IntBool::TRUE);
        assert_eq!(updated.can_cox, IntBool::TRUE);
        assert_eq!(updated.is_designated_cox, IntBool::TRUE);
    }

    #[test]
    fn promote_from_sheet_promotes_false_flags_to_true() {
        let mut conn = in_memory_conn();
        let seeded = seed_rower(
            &mut conn,
            "alice@example.com",
            Side::Port,
            false,
            false,
            false,
        );

        let updated = Rower::promote_from_sheet(
            &mut conn,
            &seeded,
            "Alice Smith",
            Side::Port,
            true,
            true,
            true,
        )
        .unwrap();

        assert_eq!(updated.can_scull, IntBool::TRUE);
        assert_eq!(updated.can_cox, IntBool::TRUE);
        assert_eq!(updated.is_designated_cox, IntBool::TRUE);
    }

    #[test]
    fn promote_from_sheet_is_noop_when_nothing_changed() {
        // Name matches, side matches, flags match → no UPDATE should
        // fire and updated_at should be unchanged.
        let mut conn = in_memory_conn();
        let seeded = seed_rower(
            &mut conn,
            "alice@example.com",
            Side::Port,
            true,
            true,
            false,
        );
        // Rename the seed so the `name` check inside promote_from_sheet
        // gets a matching string to compare against.
        let seeded_updated_at = seeded.updated_at;

        let updated = Rower::promote_from_sheet(
            &mut conn,
            &seeded,
            &seeded.name,
            Side::Port,
            true,
            true,
            false,
        )
        .unwrap();

        assert_eq!(
            updated.updated_at, seeded_updated_at,
            "no-op promote should not touch updated_at"
        );
    }

    #[test]
    fn promote_from_sheet_updates_name_unconditionally() {
        let mut conn = in_memory_conn();
        let seeded = seed_rower(
            &mut conn,
            "alice@example.com",
            Side::Port,
            false,
            true,
            false,
        );
        let original_name = seeded.name.clone();

        let updated = Rower::promote_from_sheet(
            &mut conn,
            &seeded,
            "Alice Married-Name",
            Side::Port,
            false,
            true,
            false,
        )
        .unwrap();

        assert_ne!(updated.name, original_name);
        assert_eq!(updated.name, "Alice Married-Name");
        assert_ne!(
            updated.updated_at, seeded.updated_at,
            "a name change should bump updated_at"
        );
    }
}
