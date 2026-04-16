//! Sync sources — saved configurations for importing data from
//! external systems. Each team can have multiple sync sources
//! (different sheets, different mechanisms). The `source_type`
//! determines which parser/fetcher is used; `config` holds
//! source-specific JSON configuration.

use crate::schema::sync_source;
use crate::team::TeamId;
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
    pub source_type: String,
    pub config: String,
    pub last_synced_at: Option<NaiveDateTime>,
    pub last_error: Option<String>,
    pub created_at: NaiveDateTime,
    pub poll_interval_minutes: Option<i32>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = sync_source)]
pub struct NewSyncSource {
    pub team_id: TeamId,
    pub source_type: String,
    pub config: String,
    pub poll_interval_minutes: Option<i32>,
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
        source_type: &str,
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
        source_type: &str,
        config: &str,
        poll_interval_minutes: Option<i32>,
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
                    source_type: source_type.to_string(),
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
