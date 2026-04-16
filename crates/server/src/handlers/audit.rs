//! `GET /audit` — PD-only audit log viewer with filters and pagination.

use crate::handlers::ErrorResponse;
use axum::{response::Html, Extension};
use lineup_db::app_user::{AppUser, Role, UserId};
use lineup_db::audit_log::{AuditFilter, AuditLog};
use serde::Deserialize;

use crate::handlers::internal_error;
use crate::state::TenantContext;
use crate::templates;

pub(crate) const PAGE_SIZE: i64 = 50;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct AuditQuery {
    #[serde(default)]
    pub(crate) action: Option<String>,
    #[serde(default)]
    pub(crate) user_id: Option<i32>,
    #[serde(default)]
    pub(crate) resource_type: Option<String>,
    #[serde(default)]
    pub(crate) resource_id: Option<String>,
    #[serde(default)]
    pub(crate) offset: Option<i64>,
}

/// Build the audit log markup (shared by `/audit` and `/admin/audit`).
pub(crate) async fn audit_content(
    tenant: &TenantContext,
    query: &AuditQuery,
) -> Result<maud::Markup, ErrorResponse> {
    let offset = query.offset.unwrap_or(0);
    let filter = AuditFilter {
        system_only: query.user_id == Some(-1),
        user_id: query.user_id.filter(|&id| id >= 0),
        action: query.action.clone().filter(|s| !s.is_empty()),
        resource_type: query.resource_type.clone().filter(|s| !s.is_empty()),
        resource_id: query.resource_id.clone().filter(|s| !s.is_empty()),
    };

    let (entries, actions, user_map) = tenant
        .db
        .with_conn(move |conn| {
            let entries = AuditLog::list(conn, &filter, PAGE_SIZE + 1, offset)?;
            let actions = AuditLog::distinct_actions(conn)?;
            let user_ids = AuditLog::distinct_user_ids(conn)?;
            let mut user_map = std::collections::HashMap::new();
            for uid in user_ids {
                if let Some(user) = AppUser::get(conn, UserId::new(uid))? {
                    user_map.insert(uid, user.name);
                }
            }
            Ok((entries, actions, user_map))
        })
        .await
        .map_err(internal_error)?;

    let has_more = entries.len() as i64 > PAGE_SIZE;
    let entries: Vec<AuditLog> = entries.into_iter().take(PAGE_SIZE as usize).collect();

    Ok(templates::audit::list_content(
        &entries, &actions, &user_map, query, offset, has_more,
    ))
}

/// `GET /audit/rows` — HTMX partial returning just the table rows
/// for "load more" pagination.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn rows_handler(
    Extension(tenant): Extension<TenantContext>,
    axum::extract::Query(query): axum::extract::Query<AuditQuery>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let offset = query.offset.unwrap_or(0);
    let filter = AuditFilter {
        system_only: query.user_id == Some(-1),
        user_id: query.user_id.filter(|&id| id >= 0),
        action: query.action.clone().filter(|s| !s.is_empty()),
        resource_type: query.resource_type.clone().filter(|s| !s.is_empty()),
        resource_id: query.resource_id.clone().filter(|s| !s.is_empty()),
    };

    let (entries, user_map) = tenant
        .db
        .with_conn(move |conn| {
            let entries = AuditLog::list(conn, &filter, PAGE_SIZE + 1, offset)?;
            let user_ids = AuditLog::distinct_user_ids(conn)?;
            let mut user_map = std::collections::HashMap::new();
            for uid in user_ids {
                if let Some(user) = AppUser::get(conn, UserId::new(uid))? {
                    user_map.insert(uid, user.name);
                }
            }
            Ok((entries, user_map))
        })
        .await
        .map_err(internal_error)?;

    let has_more = entries.len() as i64 > PAGE_SIZE;
    let entries: Vec<AuditLog> = entries.into_iter().take(PAGE_SIZE as usize).collect();

    Ok(Html(
        templates::audit::rows_and_load_more(&entries, &user_map, &query, offset, has_more)
            .into_string(),
    ))
}
