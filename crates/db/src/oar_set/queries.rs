use super::types::OarSetId;
use super::{
    NewOarSet, NewOarSetPreference, NewPracticeBoatOars, OarSet, OarSetPreference, PracticeBoatOars,
};
use crate::boat::types::BoatId;
use crate::practice::PracticeId;
use crate::schema::{oar_set, oar_set_preference, practice_boat_oars};
use crate::types::IntBool;
use diesel::prelude::*;
use diesel::SqliteConnection;
use std::collections::HashMap;

impl OarSet {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn insert(
        conn: &mut SqliteConnection,
        new: NewOarSet,
    ) -> Result<OarSet, diesel::result::Error> {
        diesel::insert_into(oar_set::table)
            .values(new)
            .returning(OarSet::as_returning())
            .get_result(conn)
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_all(conn: &mut SqliteConnection) -> Result<Vec<OarSet>, diesel::result::Error> {
        oar_set::table
            .select(OarSet::as_select())
            .order((oar_set::active.desc(), oar_set::name.asc()))
            .get_results(conn)
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_active(conn: &mut SqliteConnection) -> Result<Vec<OarSet>, diesel::result::Error> {
        oar_set::table
            .filter(oar_set::active.eq(IntBool::TRUE))
            .select(OarSet::as_select())
            .order(oar_set::name.asc())
            .get_results(conn)
    }

    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn get(
        conn: &mut SqliteConnection,
        id: OarSetId,
    ) -> Result<Option<OarSet>, diesel::result::Error> {
        oar_set::table
            .find(id)
            .select(OarSet::as_select())
            .first(conn)
            .optional()
    }

    #[tracing::instrument(level = "debug", skip(conn, oar_set), err)]
    pub fn save(
        conn: &mut SqliteConnection,
        oar_set: &OarSet,
    ) -> Result<OarSet, diesel::result::Error> {
        diesel::update(oar_set::table.filter(oar_set::id.eq(oar_set.id)))
            .set(oar_set)
            .returning(OarSet::as_returning())
            .get_result(conn)
    }
}

impl OarSetPreference {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn list_for_oar_set(
        conn: &mut SqliteConnection,
        oar_set_id: OarSetId,
    ) -> Result<Vec<OarSetPreference>, diesel::result::Error> {
        oar_set_preference::table
            .filter(oar_set_preference::oar_set_id.eq(oar_set_id))
            .select(OarSetPreference::as_select())
            .order(oar_set_preference::priority.asc())
            .get_results(conn)
    }

    /// Replace all preferences for an oar set with the given list.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn replace_for_oar_set(
        conn: &mut SqliteConnection,
        oar_set_id: OarSetId,
        prefs: &[(BoatId, i32)],
    ) -> Result<(), diesel::result::Error> {
        diesel::delete(
            oar_set_preference::table.filter(oar_set_preference::oar_set_id.eq(oar_set_id)),
        )
        .execute(conn)?;

        for (boat_id, priority) in prefs {
            diesel::insert_into(oar_set_preference::table)
                .values(NewOarSetPreference {
                    oar_set_id,
                    boat_id: *boat_id,
                    priority: *priority,
                })
                .execute(conn)?;
        }

        Ok(())
    }

    /// Get all preferences for active oar sets, keyed by oar_set_id.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn map_all(
        conn: &mut SqliteConnection,
    ) -> Result<HashMap<OarSetId, Vec<(BoatId, i32)>>, diesel::result::Error> {
        let rows: Vec<(OarSetId, BoatId, i32)> = oar_set_preference::table
            .inner_join(oar_set::table)
            .filter(oar_set::active.eq(IntBool::TRUE))
            .select((
                oar_set_preference::oar_set_id,
                oar_set_preference::boat_id,
                oar_set_preference::priority,
            ))
            .order(oar_set_preference::priority.asc())
            .get_results(conn)?;

        let mut map: HashMap<OarSetId, Vec<(BoatId, i32)>> = HashMap::new();
        for (oar_set_id, boat_id, priority) in rows {
            map.entry(oar_set_id).or_default().push((boat_id, priority));
        }
        Ok(map)
    }
}

impl PracticeBoatOars {
    /// Assign an oar set to a boat for a practice (upsert).
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn assign(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
        boat_id: BoatId,
        oar_set_id: OarSetId,
    ) -> Result<(), diesel::result::Error> {
        diesel::replace_into(practice_boat_oars::table)
            .values(NewPracticeBoatOars {
                practice_id,
                boat_id,
                oar_set_id,
            })
            .execute(conn)?;
        Ok(())
    }

    /// Remove oar assignment for a boat in a practice.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn unassign(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
        boat_id: BoatId,
    ) -> Result<(), diesel::result::Error> {
        diesel::delete(
            practice_boat_oars::table
                .filter(practice_boat_oars::practice_id.eq(practice_id))
                .filter(practice_boat_oars::boat_id.eq(boat_id)),
        )
        .execute(conn)?;
        Ok(())
    }

    /// Clear all oar assignments for a practice.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn clear_for_practice(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<(), diesel::result::Error> {
        diesel::delete(
            practice_boat_oars::table.filter(practice_boat_oars::practice_id.eq(practice_id)),
        )
        .execute(conn)?;
        Ok(())
    }

    /// Greedy auto-assign oar sets to boats for a practice.
    ///
    /// Clears existing assignments, then assigns greedily: boats sorted by
    /// oar demand descending (8+s first), each gets the highest-priority
    /// preferred set with enough remaining oars. Falls back to any set
    /// with enough capacity.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn auto_assign(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
        boat_ids: &[BoatId],
    ) -> Result<(), diesel::result::Error> {
        use crate::boat::Boat;

        Self::clear_for_practice(conn, practice_id)?;

        let oar_sets = OarSet::list_active(conn)?;
        let boats = Boat::list_in_service(conn)?;
        let pref_map = OarSetPreference::map_all(conn)?;

        let mut remaining: HashMap<OarSetId, i32> =
            oar_sets.iter().map(|os| (os.id, os.oar_count)).collect();

        // Sort boats by oar demand descending.
        let mut target_boats: Vec<&Boat> =
            boats.iter().filter(|b| boat_ids.contains(&b.id)).collect();
        target_boats
            .sort_by_key(|b| std::cmp::Reverse(b.seat_count.as_int() * b.oars_per_seat.as_int()));

        for boat in &target_boats {
            let oars_needed = boat.seat_count.as_int() * boat.oars_per_seat.as_int();

            // Try preferred sets first (by priority order).
            let mut assigned = false;
            for os in &oar_sets {
                if let Some(prefs) = pref_map.get(&os.id) {
                    if prefs.iter().any(|(bid, _)| *bid == boat.id) {
                        if let Some(rem) = remaining.get_mut(&os.id) {
                            if *rem >= oars_needed {
                                Self::assign(conn, practice_id, boat.id, os.id)?;
                                *rem -= oars_needed;
                                assigned = true;
                                break;
                            }
                        }
                    }
                }
            }

            if !assigned {
                let mut candidates: Vec<(OarSetId, i32, i32)> = oar_sets
                    .iter()
                    .filter_map(|os| {
                        let rem = *remaining.get(&os.id)?;
                        if rem < oars_needed {
                            return None;
                        }
                        let prio = pref_map
                            .get(&os.id)
                            .and_then(|prefs| {
                                prefs
                                    .iter()
                                    .find(|(bid, _)| *bid == boat.id)
                                    .map(|(_, p)| *p)
                            })
                            .unwrap_or(i32::MAX);
                        Some((os.id, prio, rem))
                    })
                    .collect();
                candidates.sort_by_key(|(_, prio, rem)| (*prio, std::cmp::Reverse(*rem)));

                if let Some((os_id, _, _)) = candidates.first() {
                    Self::assign(conn, practice_id, boat.id, *os_id)?;
                    if let Some(rem) = remaining.get_mut(os_id) {
                        *rem -= oars_needed;
                    }
                }
            }
        }

        Ok(())
    }

    /// All oar assignments for a practice, as a map of boat_id → oar_set_id.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn map_for_practice(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<HashMap<BoatId, OarSetId>, diesel::result::Error> {
        let rows: Vec<(BoatId, OarSetId)> = practice_boat_oars::table
            .filter(practice_boat_oars::practice_id.eq(practice_id))
            .select((practice_boat_oars::boat_id, practice_boat_oars::oar_set_id))
            .get_results(conn)?;

        Ok(rows.into_iter().collect())
    }

    /// All oar assignments for a practice with oar set details.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn list_for_practice_with_names(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<HashMap<BoatId, (OarSetId, String)>, diesel::result::Error> {
        let rows: Vec<(BoatId, OarSetId, String)> = practice_boat_oars::table
            .inner_join(oar_set::table)
            .filter(practice_boat_oars::practice_id.eq(practice_id))
            .select((
                practice_boat_oars::boat_id,
                practice_boat_oars::oar_set_id,
                oar_set::name,
            ))
            .get_results(conn)?;

        Ok(rows
            .into_iter()
            .map(|(boat_id, oar_set_id, name)| (boat_id, (oar_set_id, name)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boat::types::{CoxPosition, OarsPerSeat, SeatCount, WeightClass};
    use crate::boat::{Boat, NewBoat};
    use crate::practice::Practice;
    use crate::rower::types::Side;
    use crate::team::{NewTeam, Team, TeamId};
    use crate::test_support::in_memory_conn;

    fn setup_team(conn: &mut SqliteConnection) -> TeamId {
        let now = chrono::Utc::now().naive_utc();
        Team::create(
            conn,
            NewTeam {
                name: "Test".into(),
                created_at: now,
            },
        )
        .unwrap()
        .id
    }

    fn setup_boat(conn: &mut SqliteConnection, name: &str) -> BoatId {
        setup_boat_with_seats(conn, name, 8)
    }

    fn setup_boat_with_seats(conn: &mut SqliteConnection, name: &str, seats: i32) -> BoatId {
        Boat::insert(
            conn,
            NewBoat {
                name: name.into(),
                weight_class: WeightClass::Heavy,
                seat_count: SeatCount::new(seats),
                has_cox: if seats >= 4 {
                    IntBool::TRUE
                } else {
                    IntBool::FALSE
                },
                oars_per_seat: OarsPerSeat::new(1),
                acquired_at: None,
                manufactured_at: None,
                stroke_side: Side::Starboard,
                cox_position: CoxPosition::Stern,
            },
        )
        .unwrap()
        .id
    }

    fn make_oar(conn: &mut SqliteConnection, name: &str, count: i32) -> OarSet {
        OarSet::insert(
            conn,
            NewOarSet {
                name: name.into(),
                oar_count: count,
                notes: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn insert_and_get() {
        let mut conn = in_memory_conn();
        let oar = OarSet::insert(
            &mut conn,
            NewOarSet {
                name: "Blue".into(),
                oar_count: 8,
                notes: Some("racing oars".into()),
            },
        )
        .unwrap();

        assert_eq!(oar.name, "Blue");
        assert_eq!(oar.oar_count, 8);

        let fetched = OarSet::get(&mut conn, oar.id).unwrap().unwrap();
        assert_eq!(fetched.name, "Blue");
    }

    #[test]
    fn list_active_excludes_inactive() {
        let mut conn = in_memory_conn();

        make_oar(&mut conn, "Blue", 8);

        let mut red = make_oar(&mut conn, "Red", 4);
        red.active = IntBool::FALSE;
        OarSet::save(&mut conn, &red).unwrap();

        let active = OarSet::list_active(&mut conn).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "Blue");
    }

    #[test]
    fn preference_replace_and_list() {
        let mut conn = in_memory_conn();
        let boat_a = setup_boat(&mut conn, "A");
        let boat_b = setup_boat(&mut conn, "B");
        let oar = make_oar(&mut conn, "Blue", 8);

        OarSetPreference::replace_for_oar_set(&mut conn, oar.id, &[(boat_a, 1), (boat_b, 2)])
            .unwrap();

        let prefs = OarSetPreference::list_for_oar_set(&mut conn, oar.id).unwrap();
        assert_eq!(prefs.len(), 2);
        assert_eq!(prefs[0].boat_id, boat_a);
        assert_eq!(prefs[0].priority, 1);
    }

    #[test]
    fn practice_oar_assign_and_map() {
        let mut conn = in_memory_conn();
        let team_id = setup_team(&mut conn);
        let boat_id = setup_boat(&mut conn, "Spirit");
        let practice_id = Practice::upsert(
            &mut conn,
            team_id,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 10).unwrap(),
            None,
            None,
        )
        .unwrap()
        .id;
        let oar = make_oar(&mut conn, "Blue", 8);

        PracticeBoatOars::assign(&mut conn, practice_id, boat_id, oar.id).unwrap();

        let map = PracticeBoatOars::map_for_practice(&mut conn, practice_id).unwrap();
        assert_eq!(map.get(&boat_id), Some(&oar.id));

        // Unassign
        PracticeBoatOars::unassign(&mut conn, practice_id, boat_id).unwrap();
        let map = PracticeBoatOars::map_for_practice(&mut conn, practice_id).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn auto_assign_uses_preferences() {
        let mut conn = in_memory_conn();
        let team_id = setup_team(&mut conn);
        let eight = setup_boat_with_seats(&mut conn, "Eight", 8);
        let four = setup_boat_with_seats(&mut conn, "Four", 4);
        let practice_id = Practice::upsert(
            &mut conn,
            team_id,
            chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            None,
            None,
        )
        .unwrap()
        .id;

        let blue = make_oar(&mut conn, "Blue", 8);
        let gold = make_oar(&mut conn, "Gold", 4);

        // Blue prefers Eight, Gold prefers Four.
        OarSetPreference::replace_for_oar_set(&mut conn, blue.id, &[(eight, 0)]).unwrap();
        OarSetPreference::replace_for_oar_set(&mut conn, gold.id, &[(four, 0)]).unwrap();

        PracticeBoatOars::auto_assign(&mut conn, practice_id, &[eight, four]).unwrap();

        let map = PracticeBoatOars::map_for_practice(&mut conn, practice_id).unwrap();
        assert_eq!(map.get(&eight), Some(&blue.id));
        assert_eq!(map.get(&four), Some(&gold.id));
    }

    #[test]
    fn auto_assign_largest_boats_first() {
        let mut conn = in_memory_conn();
        let team_id = setup_team(&mut conn);
        let eight = setup_boat_with_seats(&mut conn, "Eight", 8);
        let four = setup_boat_with_seats(&mut conn, "Four", 4);
        let practice_id = Practice::upsert(
            &mut conn,
            team_id,
            chrono::NaiveDate::from_ymd_opt(2026, 6, 2).unwrap(),
            None,
            None,
        )
        .unwrap()
        .id;

        // Only one set with 8 oars — the eight should get it, not the four.
        let blue = make_oar(&mut conn, "Blue", 8);

        PracticeBoatOars::auto_assign(&mut conn, practice_id, &[four, eight]).unwrap();

        let map = PracticeBoatOars::map_for_practice(&mut conn, practice_id).unwrap();
        assert_eq!(map.get(&eight), Some(&blue.id));
        // Four gets nothing — Blue only has 8 oars, 8 are used by Eight.
        assert!(map.get(&four).is_none());
    }

    #[test]
    fn auto_assign_splits_oar_set() {
        let mut conn = in_memory_conn();
        let team_id = setup_team(&mut conn);
        let four_a = setup_boat_with_seats(&mut conn, "FourA", 4);
        let four_b = setup_boat_with_seats(&mut conn, "FourB", 4);
        let practice_id = Practice::upsert(
            &mut conn,
            team_id,
            chrono::NaiveDate::from_ymd_opt(2026, 6, 3).unwrap(),
            None,
            None,
        )
        .unwrap()
        .id;

        // One set with 8 oars — enough for two 4+s.
        let blue = make_oar(&mut conn, "Blue", 8);

        PracticeBoatOars::auto_assign(&mut conn, practice_id, &[four_a, four_b]).unwrap();

        let map = PracticeBoatOars::map_for_practice(&mut conn, practice_id).unwrap();
        assert_eq!(map.get(&four_a), Some(&blue.id));
        assert_eq!(map.get(&four_b), Some(&blue.id));
    }
}
