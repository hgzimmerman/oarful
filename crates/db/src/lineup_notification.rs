//! Per-rower notification tracking for committed lineups.
//!
//! Records which rowers have been notified about their placement in a
//! practice lineup. Used to derive the Notified phase and to detect
//! rowers who haven't been notified after lineup edits.

use crate::practice::PracticeId;
use crate::rower::types::RowerId;
use crate::schema::lineup_notification;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::SqliteConnection;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = lineup_notification)]
pub struct LineupNotification {
    pub id: i32,
    pub practice_id: PracticeId,
    pub rower_id: RowerId,
    pub sent_at: NaiveDateTime,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = lineup_notification)]
pub struct NewLineupNotification {
    pub practice_id: PracticeId,
    pub rower_id: RowerId,
    pub sent_at: NaiveDateTime,
}

impl LineupNotification {
    /// Record that a rower was notified about their lineup placement.
    /// Uses INSERT OR REPLACE so re-sends update the timestamp.
    pub fn record(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
        rower_id: RowerId,
        sent_at: NaiveDateTime,
    ) -> Result<(), diesel::result::Error> {
        diesel::replace_into(lineup_notification::table)
            .values(NewLineupNotification {
                practice_id,
                rower_id,
                sent_at,
            })
            .execute(conn)?;
        Ok(())
    }

    /// Record notifications for multiple rowers at once.
    pub fn record_batch(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
        rower_ids: &[RowerId],
        sent_at: NaiveDateTime,
    ) -> Result<(), diesel::result::Error> {
        if rower_ids.is_empty() {
            return Ok(());
        }
        let rows: Vec<NewLineupNotification> = rower_ids
            .iter()
            .map(|&rower_id| NewLineupNotification {
                practice_id,
                rower_id,
                sent_at,
            })
            .collect();
        diesel::replace_into(lineup_notification::table)
            .values(&rows)
            .execute(conn)?;
        Ok(())
    }

    /// Get all notified rower IDs for a practice.
    pub fn notified_rowers(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<Vec<RowerId>, diesel::result::Error> {
        lineup_notification::table
            .filter(lineup_notification::practice_id.eq(practice_id))
            .select(lineup_notification::rower_id)
            .load(conn)
    }

    /// Check whether all the given rower IDs have been notified for a practice.
    pub fn all_notified(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
        boated_rower_ids: &[RowerId],
    ) -> Result<bool, diesel::result::Error> {
        if boated_rower_ids.is_empty() {
            return Ok(false);
        }
        let notified: std::collections::HashSet<RowerId> =
            Self::notified_rowers(conn, practice_id)?
                .into_iter()
                .collect();
        Ok(boated_rower_ids.iter().all(|id| notified.contains(id)))
    }

    /// Get notified rower IDs across multiple practices in one query.
    /// Returns a map of practice_id → set of notified rower IDs.
    pub fn notified_rowers_for_practices(
        conn: &mut SqliteConnection,
        practice_ids: &[PracticeId],
    ) -> Result<
        std::collections::HashMap<PracticeId, std::collections::HashSet<RowerId>>,
        diesel::result::Error,
    > {
        let rows: Vec<(PracticeId, RowerId)> = lineup_notification::table
            .filter(lineup_notification::practice_id.eq_any(practice_ids))
            .select((
                lineup_notification::practice_id,
                lineup_notification::rower_id,
            ))
            .load(conn)?;
        let mut map: std::collections::HashMap<PracticeId, std::collections::HashSet<RowerId>> =
            std::collections::HashMap::new();
        for (pid, rid) in rows {
            map.entry(pid).or_default().insert(rid);
        }
        Ok(map)
    }

    /// Delete all notifications for a practice (e.g., after lineup re-commit).
    pub fn clear_for_practice(
        conn: &mut SqliteConnection,
        practice_id: PracticeId,
    ) -> Result<usize, diesel::result::Error> {
        diesel::delete(
            lineup_notification::table.filter(lineup_notification::practice_id.eq(practice_id)),
        )
        .execute(conn)
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
    use crate::team::{NewTeam, Team};
    use crate::test_support::in_memory_conn;
    use crate::types::IntBool;

    fn seed(conn: &mut diesel::SqliteConnection) -> (crate::team::TeamId, PracticeId, RowerId) {
        let now = chrono::Utc::now().naive_utc();
        let tid = Team::create(
            conn,
            NewTeam {
                name: "T".into(),
                created_at: now,
            },
        )
        .unwrap()
        .id;
        let pid = Practice::upsert(
            conn,
            tid,
            chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            None,
            None,
        )
        .unwrap()
        .id;
        let rid = Rower::insert(
            conn,
            NewRower {
                name: "R".into(),
                first_name: None,
                last_name: None,
                weight_class: RowerWeightClass::Medium,
                skill: Skill::Intermediate,
                strength: Strength::Intermediate,
                height: Height::Medium,
                side: Side::Port,
                side_strength: SideStrength::default(),
                sweep_bias: SweepBias::default(),
                can_cox: IntBool::FALSE,
                is_designated_cox: IntBool::FALSE,
                active: IntBool::TRUE,
                created_at: now,
                updated_at: now,
            },
        )
        .unwrap()
        .id;
        (tid, pid, rid)
    }

    #[test]
    fn record_batch_and_query() {
        let mut conn = in_memory_conn();
        let (_, pid, rid) = seed(&mut conn);
        let now = chrono::Utc::now().naive_utc();

        // Second rower for batch test.
        let rid2 = Rower::insert(
            &mut conn,
            NewRower {
                name: "R2".into(),
                first_name: None,
                last_name: None,
                weight_class: RowerWeightClass::Medium,
                skill: Skill::Intermediate,
                strength: Strength::Intermediate,
                height: Height::Medium,
                side: Side::Starboard,
                side_strength: SideStrength::default(),
                sweep_bias: SweepBias::default(),
                can_cox: IntBool::FALSE,
                is_designated_cox: IntBool::FALSE,
                active: IntBool::TRUE,
                created_at: now,
                updated_at: now,
            },
        )
        .unwrap()
        .id;

        LineupNotification::record_batch(&mut conn, pid, &[rid, rid2], now).unwrap();

        let notified = LineupNotification::notified_rowers(&mut conn, pid).unwrap();
        assert_eq!(notified.len(), 2);
        assert!(notified.contains(&rid));
        assert!(notified.contains(&rid2));

        assert!(LineupNotification::all_notified(&mut conn, pid, &[rid, rid2]).unwrap());
        assert!(!LineupNotification::all_notified(
            &mut conn,
            pid,
            &[rid, rid2, crate::rower::types::RowerId::new(999)]
        )
        .unwrap());
    }

    #[test]
    fn clear_for_practice_removes_all() {
        let mut conn = in_memory_conn();
        let (_, pid, rid) = seed(&mut conn);
        let now = chrono::Utc::now().naive_utc();

        LineupNotification::record(&mut conn, pid, rid, now).unwrap();
        assert_eq!(
            LineupNotification::notified_rowers(&mut conn, pid)
                .unwrap()
                .len(),
            1
        );

        let deleted = LineupNotification::clear_for_practice(&mut conn, pid).unwrap();
        assert_eq!(deleted, 1);
        assert!(LineupNotification::notified_rowers(&mut conn, pid)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn record_batch_empty_is_noop() {
        let mut conn = in_memory_conn();
        let (_, pid, _) = seed(&mut conn);
        let now = chrono::Utc::now().naive_utc();
        LineupNotification::record_batch(&mut conn, pid, &[], now).unwrap();
        assert!(LineupNotification::notified_rowers(&mut conn, pid)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn record_updates_timestamp_on_conflict() {
        let mut conn = in_memory_conn();
        let (_, pid, rid) = seed(&mut conn);
        let t1 = chrono::NaiveDateTime::parse_from_str("2026-06-01 10:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap();
        let t2 = chrono::NaiveDateTime::parse_from_str("2026-06-01 12:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap();

        LineupNotification::record(&mut conn, pid, rid, t1).unwrap();
        LineupNotification::record(&mut conn, pid, rid, t2).unwrap();

        // Should still be just one row.
        let notified = LineupNotification::notified_rowers(&mut conn, pid).unwrap();
        assert_eq!(notified.len(), 1);
    }
}
