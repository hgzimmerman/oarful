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
            .on_conflict((availability::rower_id, availability::practice_id))
            .do_update()
            .set(availability::status.eq(new.status))
            .execute(conn)?;
        Ok(())
    }

    /// Remove a rower's availability record for a practice (revert to "no response").
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn delete(
        conn: &mut SqliteConnection,
        rower_id: RowerId,
        practice_id: PracticeId,
    ) -> Result<(), diesel::result::Error> {
        diesel::delete(
            availability::table
                .filter(availability::rower_id.eq(rower_id))
                .filter(availability::practice_id.eq(practice_id)),
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::practice::Practice;
    use crate::rower::types::{
        Height, RowerWeightClass, Side, SideStrength, Skill, Strength, SweepBias,
    };
    use crate::rower::{NewRower, Rower};
    use crate::team::{NewTeam, Team, TeamId};
    use crate::test_support::in_memory_conn;
    use crate::types::IntBool;

    fn seed_team(conn: &mut diesel::SqliteConnection) -> TeamId {
        let now = chrono::Utc::now().naive_utc();
        Team::create(
            conn,
            NewTeam {
                name: "T".into(),
                created_at: now,
            },
        )
        .unwrap()
        .id
    }

    fn seed_rower(conn: &mut diesel::SqliteConnection) -> Rower {
        let now = chrono::Utc::now().naive_utc();
        Rower::insert(
            conn,
            NewRower {
                name: "R".into(),
                weight_class: RowerWeightClass::Medium,
                skill: Skill::Intermediate,
                strength: Strength::Intermediate,
                height: Height::Medium,
                side: Side::Port,
                side_strength: SideStrength::default(),
                sweep_bias: SweepBias::default(),
                can_cox: IntBool::TRUE,
                is_designated_cox: IntBool::FALSE,
                active: IntBool::TRUE,
                created_at: now,
                updated_at: now,
            },
        )
        .unwrap()
    }

    #[test]
    fn upsert_and_list() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let r = seed_rower(&mut conn);
        let p = Practice::upsert(
            &mut conn,
            tid,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            None,
            None,
        )
        .unwrap();

        Availability::upsert(
            &mut conn,
            NewAvailability {
                rower_id: r.id,
                practice_id: p.id,
                status: AvailabilityStatus::Yes,
            },
        )
        .unwrap();

        let list = Availability::list_for_practice(&mut conn, p.id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].status, AvailabilityStatus::Yes);
    }

    #[test]
    fn upsert_updates_status() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let r = seed_rower(&mut conn);
        let p = Practice::upsert(
            &mut conn,
            tid,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            None,
            None,
        )
        .unwrap();

        Availability::upsert(
            &mut conn,
            NewAvailability {
                rower_id: r.id,
                practice_id: p.id,
                status: AvailabilityStatus::Yes,
            },
        )
        .unwrap();
        Availability::upsert(
            &mut conn,
            NewAvailability {
                rower_id: r.id,
                practice_id: p.id,
                status: AvailabilityStatus::No,
            },
        )
        .unwrap();

        let map = Availability::map_for_practice(&mut conn, p.id).unwrap();
        assert_eq!(map[&r.id], AvailabilityStatus::No);
    }

    #[test]
    fn delete_removes() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let r = seed_rower(&mut conn);
        let p = Practice::upsert(
            &mut conn,
            tid,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            None,
            None,
        )
        .unwrap();

        Availability::upsert(
            &mut conn,
            NewAvailability {
                rower_id: r.id,
                practice_id: p.id,
                status: AvailabilityStatus::Yes,
            },
        )
        .unwrap();
        Availability::delete(&mut conn, r.id, p.id).unwrap();

        let list = Availability::list_for_practice(&mut conn, p.id).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn map_for_practices() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let r = seed_rower(&mut conn);
        let p1 = Practice::upsert(
            &mut conn,
            tid,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            None,
            None,
        )
        .unwrap();
        let p2 = Practice::upsert(
            &mut conn,
            tid,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 2).unwrap(),
            None,
            None,
        )
        .unwrap();

        Availability::upsert(
            &mut conn,
            NewAvailability {
                rower_id: r.id,
                practice_id: p1.id,
                status: AvailabilityStatus::Yes,
            },
        )
        .unwrap();
        Availability::upsert(
            &mut conn,
            NewAvailability {
                rower_id: r.id,
                practice_id: p2.id,
                status: AvailabilityStatus::No,
            },
        )
        .unwrap();

        let map = Availability::map_for_practices(&mut conn, &[p1.id, p2.id]).unwrap();
        assert_eq!(map[&(r.id, p1.id)], AvailabilityStatus::Yes);
        assert_eq!(map[&(r.id, p2.id)], AvailabilityStatus::No);
    }

    #[test]
    fn practices_with_responses() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let r = seed_rower(&mut conn);
        let p1 = Practice::upsert(
            &mut conn,
            tid,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            None,
            None,
        )
        .unwrap();
        let p2 = Practice::upsert(
            &mut conn,
            tid,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 2).unwrap(),
            None,
            None,
        )
        .unwrap();

        Availability::upsert(
            &mut conn,
            NewAvailability {
                rower_id: r.id,
                practice_id: p1.id,
                status: AvailabilityStatus::Yes,
            },
        )
        .unwrap();

        let with = Availability::practices_with_responses(&mut conn, &[p1.id, p2.id]).unwrap();
        assert_eq!(with.len(), 1);
        assert_eq!(with[0], p1.id);
    }
}
