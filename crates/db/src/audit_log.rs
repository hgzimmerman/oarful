//! Audit log — records who changed what and when. Entries older than
//! the retention window are periodically pruned.

use crate::app_user::UserId;
use crate::schema::audit_log;
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
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = audit_log)]
pub struct NewAuditEntry {
    pub timestamp: NaiveDateTime,
    pub user_id: Option<UserId>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub detail: Option<String>,
}

/// Filter criteria for querying the audit log.
#[derive(Debug, Default)]
pub struct AuditFilter {
    pub user_id: Option<UserId>,
    /// If true, only show entries with user_id IS NULL (system actions).
    pub system_only: bool,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
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
    ) -> Result<Vec<String>, diesel::result::Error> {
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
