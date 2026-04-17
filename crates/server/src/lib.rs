//! lineup_server — coach-facing HTTP UI for the lineup generator.
//!
//! A thin axum web server that wraps [`lineup_solver::solve`] and
//! [`lineup_db`] in a server-rendered Maud + HTMX frontend. See
//! `DESIGN.md` in this crate for the architecture rationale.

use axum::Router;
use tower_http::services::ServeDir;

pub(crate) mod audit;
pub(crate) mod extract;
pub(crate) mod handlers;
pub(crate) mod jwt;
pub(crate) mod magic_link;
pub mod mailer;
pub(crate) mod request_id;
pub(crate) mod state;
pub(crate) mod templates;
pub(crate) mod tenant_cache;
pub(crate) mod unsubscribe;

pub(crate) use state::AppState;

/// Build the full application router.
///
/// `master_conn_str` points at the global master.db (tenant registry).
/// `data_dir` is the base directory for per-tenant SQLite files.
pub fn build_router(
    master_conn_str: &str,
    data_dir: &str,
    public_dir: &str,
    mailer: std::sync::Arc<dyn mailer::Mailer>,
) -> anyhow::Result<Router> {
    let state = AppState::new(master_conn_str, data_dir, mailer)?;

    // Run demo cleanup at startup, then periodically.
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        // Startup sweep.
        handlers::demo::cleanup_expired_demos(&cleanup_state).await;
        // Then every hour.
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        interval.tick().await; // skip the immediate first tick
        loop {
            interval.tick().await;
            handlers::demo::cleanup_expired_demos(&cleanup_state).await;
        }
    });

    // Periodic sync polling — check every 5 minutes for sources that
    // need re-syncing. Each source tracks its own interval internally.
    let sync_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            handlers::sync::poll_sync_sources(&sync_state).await;
        }
    });

    // Audit log cleanup — prune entries older than 90 days, daily.
    let audit_state = state.clone();
    tokio::spawn(async move {
        audit::cleanup_all(&audit_state).await;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(86400));
        interval.tick().await; // skip immediate tick
        loop {
            interval.tick().await;
            audit::cleanup_all(&audit_state).await;
        }
    });

    Ok(handlers::create_router(state)
        .fallback_service(ServeDir::new(public_dir))
        .layer(axum::middleware::from_fn(request_id::request_tracing)))
}
