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
        let rows: Vec<NewLineupNotification> = rower_ids
            .iter()
            .map(|&rower_id| NewLineupNotification {
                practice_id,
                rower_id,
                sent_at,
            })
            .collect();
        for row in &rows {
            diesel::replace_into(lineup_notification::table)
                .values(row)
                .execute(conn)?;
        }
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
