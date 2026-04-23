//! Sync sources — saved configurations for importing data from
//! external systems. Each team can have multiple sync sources
//! (different sheets, different mechanisms). The `source_type`
//! determines which parser/fetcher is used; `config` holds
//! source-specific JSON configuration.

use crate::schema::sync_source;
use crate::team::TeamId;
use crate::types::{DurationMinutes, SyncSourceType};
use chrono::NaiveDateTime;
use diesel::prelude::*;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct SyncSourceId(i32);

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = sync_source)]
pub struct SyncSource {
    pub id: SyncSourceId,
    pub team_id: TeamId,
    pub source_type: SyncSourceType,
    pub config: String,
    pub last_synced_at: Option<NaiveDateTime>,
    pub last_error: Option<String>,
    pub created_at: NaiveDateTime,
    pub poll_interval_minutes: Option<DurationMinutes>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = sync_source)]
pub struct NewSyncSource {
    pub team_id: TeamId,
    pub source_type: SyncSourceType,
    pub config: String,
    pub poll_interval_minutes: Option<DurationMinutes>,
}

impl SyncSource {
    /// All sync sources for a team.
    pub fn list_for_team(
        conn: &mut SqliteConnection,
        team_id: TeamId,
    ) -> Result<Vec<SyncSource>, diesel::result::Error> {
        sync_source::table
            .filter(sync_source::team_id.eq(team_id))
            .select(SyncSource::as_select())
            .order(sync_source::created_at.asc())
            .get_results(conn)
    }

    /// Find a specific sync source by team and source type.
    pub fn find_by_type(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        source_type: &SyncSourceType,
    ) -> Result<Option<SyncSource>, diesel::result::Error> {
        sync_source::table
            .filter(sync_source::team_id.eq(team_id))
            .filter(sync_source::source_type.eq(source_type))
            .select(SyncSource::as_select())
            .first(conn)
            .optional()
    }

    /// Create or update a sync source by (team, source_type).
    /// Upserts the config and clears any previous error.
    pub fn upsert(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        source_type: &SyncSourceType,
        config: &str,
        poll_interval_minutes: Option<DurationMinutes>,
    ) -> Result<(), diesel::result::Error> {
        if let Some(existing) = Self::find_by_type(conn, team_id, source_type)? {
            diesel::update(sync_source::table.find(existing.id))
                .set((
                    sync_source::config.eq(config),
                    sync_source::last_error.eq(None::<String>),
                    sync_source::poll_interval_minutes.eq(poll_interval_minutes),
                ))
                .execute(conn)?;
        } else {
            diesel::insert_into(sync_source::table)
                .values(NewSyncSource {
                    team_id,
                    source_type: source_type.clone(),
                    config: config.to_string(),
                    poll_interval_minutes,
                })
                .execute(conn)?;
        }
        Ok(())
    }

    /// All sync sources across all teams that have polling enabled.
    pub fn list_pollable(
        conn: &mut SqliteConnection,
    ) -> Result<Vec<SyncSource>, diesel::result::Error> {
        sync_source::table
            .filter(sync_source::poll_interval_minutes.is_not_null())
            .select(SyncSource::as_select())
            .get_results(conn)
    }

    /// Record a successful sync timestamp.
    pub fn mark_synced(
        conn: &mut SqliteConnection,
        id: SyncSourceId,
    ) -> Result<(), diesel::result::Error> {
        let now = chrono::Utc::now().naive_utc();
        diesel::update(sync_source::table.find(id))
            .set((
                sync_source::last_synced_at.eq(Some(now)),
                sync_source::last_error.eq(None::<String>),
            ))
            .execute(conn)?;
        Ok(())
    }

    /// Record a sync error.
    pub fn mark_error(
        conn: &mut SqliteConnection,
        id: SyncSourceId,
        error: &str,
    ) -> Result<(), diesel::result::Error> {
        diesel::update(sync_source::table.find(id))
            .set(sync_source::last_error.eq(Some(error)))
            .execute(conn)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team::{NewTeam, Team};
    use crate::test_support::in_memory_conn;

    fn seed_team(conn: &mut diesel::SqliteConnection) -> TeamId {
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

    #[test]
    fn upsert_creates_then_updates() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let st = SyncSourceType::new("google_sheet");

        SyncSource::upsert(&mut conn, tid, &st, r#"{"id":"abc"}"#, None).unwrap();
        let found = SyncSource::find_by_type(&mut conn, tid, &st)
            .unwrap()
            .unwrap();
        assert_eq!(found.config, r#"{"id":"abc"}"#);

        // Upsert updates config
        SyncSource::upsert(&mut conn, tid, &st, r#"{"id":"xyz"}"#, None).unwrap();
        let found = SyncSource::find_by_type(&mut conn, tid, &st)
            .unwrap()
            .unwrap();
        assert_eq!(found.config, r#"{"id":"xyz"}"#);
    }

    #[test]
    fn list_for_team() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let tid2 = {
            let now = chrono::Utc::now().naive_utc();
            Team::create(
                &mut conn,
                NewTeam {
                    name: "Other".into(),
                    created_at: now,
                },
            )
            .unwrap()
            .id
        };
        let gs = SyncSourceType::new("google_sheet");

        SyncSource::upsert(&mut conn, tid, &gs, r#"{"id":"a"}"#, None).unwrap();
        SyncSource::upsert(&mut conn, tid2, &gs, r#"{"id":"b"}"#, None).unwrap();

        let list = SyncSource::list_for_team(&mut conn, tid).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].config, r#"{"id":"a"}"#);
    }

    #[test]
    fn list_pollable() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let gs = SyncSourceType::new("google_sheet");

        SyncSource::upsert(&mut conn, tid, &gs, "{}", Some(DurationMinutes::new(30))).unwrap();

        let pollable = SyncSource::list_pollable(&mut conn).unwrap();
        assert_eq!(pollable.len(), 1);
        assert!(pollable[0].poll_interval_minutes.is_some());
    }

    #[test]
    fn mark_synced_and_error() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let st = SyncSourceType::new("google_sheet");
        SyncSource::upsert(&mut conn, tid, &st, "{}", None).unwrap();
        let src = SyncSource::find_by_type(&mut conn, tid, &st)
            .unwrap()
            .unwrap();
        assert!(src.last_synced_at.is_none());
        assert!(src.last_error.is_none());

        SyncSource::mark_synced(&mut conn, src.id).unwrap();
        let src = SyncSource::find_by_type(&mut conn, tid, &st)
            .unwrap()
            .unwrap();
        assert!(src.last_synced_at.is_some());
        assert!(src.last_error.is_none());

        SyncSource::mark_error(&mut conn, src.id, "timeout").unwrap();
        let src = SyncSource::find_by_type(&mut conn, tid, &st)
            .unwrap()
            .unwrap();
        assert_eq!(src.last_error.as_deref(), Some("timeout"));

        // mark_synced clears error
        SyncSource::mark_synced(&mut conn, src.id).unwrap();
        let src = SyncSource::find_by_type(&mut conn, tid, &st)
            .unwrap()
            .unwrap();
        assert!(src.last_error.is_none());
    }

    #[test]
    fn upsert_clears_error() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let st = SyncSourceType::new("google_sheet");
        SyncSource::upsert(&mut conn, tid, &st, "{}", None).unwrap();
        let src = SyncSource::find_by_type(&mut conn, tid, &st)
            .unwrap()
            .unwrap();
        SyncSource::mark_error(&mut conn, src.id, "oops").unwrap();

        // Re-upsert should clear error
        SyncSource::upsert(&mut conn, tid, &st, r#"{"new":true}"#, None).unwrap();
        let src = SyncSource::find_by_type(&mut conn, tid, &st)
            .unwrap()
            .unwrap();
        assert!(src.last_error.is_none());
    }
}
