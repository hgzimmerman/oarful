//! Audit log — records who changed what and when. Entries older than
//! the retention window are periodically pruned.

use crate::app_user::UserId;
use crate::schema::audit_log;
use crate::types::{AuditAction, AuditResourceId, AuditResourceType};
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::SqliteConnection;
use serde::{Deserialize, Serialize};

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct AuditLogId(i32);

impl AuditLogId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for AuditLogId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for AuditLogId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = audit_log)]
pub struct AuditLog {
    pub id: AuditLogId,
    pub timestamp: NaiveDateTime,
    pub user_id: Option<UserId>,
    pub action: AuditAction,
    pub resource_type: AuditResourceType,
    pub resource_id: AuditResourceId,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = audit_log)]
pub struct NewAuditEntry {
    pub timestamp: NaiveDateTime,
    pub user_id: Option<UserId>,
    pub action: AuditAction,
    pub resource_type: AuditResourceType,
    pub resource_id: AuditResourceId,
    pub detail: Option<String>,
}

/// Filter criteria for querying the audit log.
#[derive(Debug, Default)]
pub struct AuditFilter {
    pub user_id: Option<UserId>,
    /// If true, only show entries with user_id IS NULL (system actions).
    pub system_only: bool,
    pub action: Option<AuditAction>,
    pub resource_type: Option<AuditResourceType>,
    pub resource_id: Option<AuditResourceId>,
}

impl AuditLog {
    /// Insert one audit entry.
    pub fn record(
        conn: &mut SqliteConnection,
        entry: NewAuditEntry,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(audit_log::table)
            .values(&entry)
            .execute(conn)?;
        Ok(())
    }

    /// Query entries with optional filters, newest first, with offset/limit.
    pub fn list(
        conn: &mut SqliteConnection,
        filter: &AuditFilter,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AuditLog>, diesel::result::Error> {
        let mut query = audit_log::table
            .select(AuditLog::as_select())
            .order(audit_log::timestamp.desc())
            .into_boxed();

        if filter.system_only {
            query = query.filter(audit_log::user_id.is_null());
        } else if let Some(uid) = filter.user_id {
            query = query.filter(audit_log::user_id.eq(uid));
        }
        if let Some(ref action) = filter.action {
            query = query.filter(audit_log::action.eq(action));
        }
        if let Some(ref rt) = filter.resource_type {
            query = query.filter(audit_log::resource_type.eq(rt));
        }
        if let Some(ref rid) = filter.resource_id {
            query = query.filter(audit_log::resource_id.eq(rid));
        }

        query.limit(limit).offset(offset).get_results(conn)
    }

    /// Distinct action values currently in the log.
    pub fn distinct_actions(
        conn: &mut SqliteConnection,
    ) -> Result<Vec<AuditAction>, diesel::result::Error> {
        audit_log::table
            .select(audit_log::action)
            .distinct()
            .order(audit_log::action.asc())
            .get_results(conn)
    }

    /// Distinct (user_id) values currently in the log (non-null only).
    pub fn distinct_user_ids(
        conn: &mut SqliteConnection,
    ) -> Result<Vec<UserId>, diesel::result::Error> {
        audit_log::table
            .select(audit_log::user_id)
            .filter(audit_log::user_id.is_not_null())
            .distinct()
            .get_results::<Option<UserId>>(conn)
            .map(|v| v.into_iter().flatten().collect())
    }

    /// Delete entries older than `cutoff`. Returns the number of rows deleted.
    pub fn prune_before(
        conn: &mut SqliteConnection,
        cutoff: NaiveDateTime,
    ) -> Result<usize, diesel::result::Error> {
        diesel::delete(audit_log::table.filter(audit_log::timestamp.lt(cutoff))).execute(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_user::{AppUser, NewAppUser, UserStatus};
    use crate::test_support::in_memory_conn;

    fn ts(secs: i64) -> NaiveDateTime {
        chrono::DateTime::from_timestamp(secs, 0)
            .unwrap()
            .naive_utc()
    }

    fn seed_user(conn: &mut diesel::SqliteConnection, email: &str) -> UserId {
        let now = chrono::Utc::now().naive_utc();
        AppUser::create(
            conn,
            NewAppUser {
                email: email.into(),
                password_hash: None,
                name: "U".into(),
                status: UserStatus::Active,
                created_at: now,
                updated_at: now,
            },
        )
        .unwrap()
        .id
    }

    fn seed_entry(
        conn: &mut diesel::SqliteConnection,
        user_id: Option<UserId>,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        timestamp: NaiveDateTime,
    ) {
        AuditLog::record(
            conn,
            NewAuditEntry {
                timestamp,
                user_id,
                action: AuditAction::new(action),
                resource_type: AuditResourceType::new(resource_type),
                resource_id: AuditResourceId::new(resource_id),
                detail: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn record_and_list_unfiltered() {
        let mut conn = in_memory_conn();
        let u1 = seed_user(&mut conn, "a@test.com");
        let u2 = seed_user(&mut conn, "b@test.com");
        seed_entry(&mut conn, Some(u1), "boat.create", "boat", "1", ts(1000));
        seed_entry(&mut conn, Some(u2), "rower.update", "rower", "5", ts(2000));

        let all = AuditLog::list(&mut conn, &AuditFilter::default(), 100, 0).unwrap();
        assert_eq!(all.len(), 2);
        // Newest first
        assert_eq!(all[0].action.as_str(), "rower.update");
        assert_eq!(all[1].action.as_str(), "boat.create");
    }

    #[test]
    fn list_filter_by_user() {
        let mut conn = in_memory_conn();
        let u1 = seed_user(&mut conn, "a@test.com");
        let u2 = seed_user(&mut conn, "b@test.com");
        seed_entry(&mut conn, Some(u1), "a", "r", "1", ts(1000));
        seed_entry(&mut conn, Some(u2), "b", "r", "2", ts(2000));

        let filter = AuditFilter {
            user_id: Some(u1),
            ..Default::default()
        };
        let results = AuditLog::list(&mut conn, &filter, 100, 0).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].user_id, Some(u1));
    }

    #[test]
    fn list_filter_system_only() {
        let mut conn = in_memory_conn();
        let u1 = seed_user(&mut conn, "a@test.com");
        seed_entry(&mut conn, Some(u1), "user.action", "r", "1", ts(1000));
        seed_entry(&mut conn, None, "system.sync", "r", "2", ts(2000));

        let filter = AuditFilter {
            system_only: true,
            ..Default::default()
        };
        let results = AuditLog::list(&mut conn, &filter, 100, 0).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action.as_str(), "system.sync");
    }

    #[test]
    fn list_filter_by_action() {
        let mut conn = in_memory_conn();
        seed_entry(&mut conn, None, "boat.create", "boat", "1", ts(1000));
        seed_entry(&mut conn, None, "rower.update", "rower", "1", ts(2000));

        let filter = AuditFilter {
            action: Some(AuditAction::new("boat.create")),
            ..Default::default()
        };
        let results = AuditLog::list(&mut conn, &filter, 100, 0).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn list_limit_and_offset() {
        let mut conn = in_memory_conn();
        for i in 0..5 {
            seed_entry(&mut conn, None, "a", "r", &i.to_string(), ts(i * 1000));
        }
        let page = AuditLog::list(&mut conn, &AuditFilter::default(), 2, 1).unwrap();
        assert_eq!(page.len(), 2);
    }

    #[test]
    fn distinct_actions() {
        let mut conn = in_memory_conn();
        seed_entry(&mut conn, None, "boat.create", "boat", "1", ts(1000));
        seed_entry(&mut conn, None, "boat.create", "boat", "2", ts(2000));
        seed_entry(&mut conn, None, "rower.update", "rower", "1", ts(3000));

        let actions = AuditLog::distinct_actions(&mut conn).unwrap();
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn distinct_user_ids() {
        let mut conn = in_memory_conn();
        let u1 = seed_user(&mut conn, "a@test.com");
        let u2 = seed_user(&mut conn, "b@test.com");
        seed_entry(&mut conn, Some(u1), "a", "r", "1", ts(1000));
        seed_entry(&mut conn, Some(u1), "b", "r", "2", ts(2000));
        seed_entry(&mut conn, Some(u2), "c", "r", "3", ts(3000));
        seed_entry(&mut conn, None, "d", "r", "4", ts(4000)); // system

        let ids = AuditLog::distinct_user_ids(&mut conn).unwrap();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn prune_before() {
        let mut conn = in_memory_conn();
        seed_entry(&mut conn, None, "old", "r", "1", ts(1000));
        seed_entry(&mut conn, None, "new", "r", "2", ts(5000));

        let deleted = AuditLog::prune_before(&mut conn, ts(3000)).unwrap();
        assert_eq!(deleted, 1);

        let remaining = AuditLog::list(&mut conn, &AuditFilter::default(), 100, 0).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].action.as_str(), "new");
    }
}
