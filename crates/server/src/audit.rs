//! Fire-and-forget audit log writes + periodic cleanup.

use lineup_db::audit_log::{AuditLog, NewAuditEntry};
use lineup_db::state::Db;

/// Fire-and-forget audit log write. Spawns a background task so the
/// handler response is never blocked by audit I/O. Failures are logged
/// but never propagated.
pub(crate) fn record(
    db: &Db,
    user_id: Option<i32>,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    detail: Option<String>,
) {
    let db = db.clone();
    let entry = NewAuditEntry {
        timestamp: chrono::Utc::now().naive_utc(),
        user_id,
        action: action.to_string(),
        resource_type: resource_type.to_string(),
        resource_id: resource_id.to_string(),
        detail,
    };
    tokio::spawn(async move {
        if let Err(e) = db
            .with_conn(move |conn| AuditLog::record(conn, entry))
            .await
        {
            tracing::warn!(?e, "audit log write failed");
        }
    });
}

/// Delete audit entries older than 90 days across all non-demo tenants.
pub(crate) async fn cleanup_all(state: &crate::AppState) {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(90)).naive_utc();
    let tenants = match state
        .master_db
        .with_conn(|conn| lineup_master_db::tenant::Tenant::list_all(conn))
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(?e, "audit cleanup: failed to list tenants");
            return;
        }
    };
    for tenant in &tenants {
        if tenant.demo_expires_at.is_some() {
            continue;
        }
        let (db, _) = match state.tenant_db(tenant.id).await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(tenant_id = %tenant.id, ?e, "audit cleanup: failed to open DB");
                continue;
            }
        };
        match db
            .with_conn(move |conn| AuditLog::prune_before(conn, cutoff))
            .await
        {
            Ok(deleted) if deleted > 0 => {
                tracing::info!(tenant_id = %tenant.id, deleted, "audit cleanup: pruned old entries");
            }
            Err(e) => {
                tracing::warn!(tenant_id = %tenant.id, ?e, "audit cleanup: prune failed");
            }
            _ => {}
        }
    }
}
