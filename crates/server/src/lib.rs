//! lineup_server — coach-facing HTTP UI for the lineup generator.
//!
//! A thin axum web server that wraps [`lineup_solver::solve`] and
//! [`lineup_db`] in a server-rendered Maud + HTMX frontend. See
//! `DESIGN.md` in this crate for the architecture rationale.

use axum::Router;
use tower_http::services::ServeDir;

pub(crate) mod extract;
pub(crate) mod handlers;
pub(crate) mod jwt;
pub(crate) mod magic_link;
pub mod mailer;
pub(crate) mod request_id;
pub(crate) mod state;
pub(crate) mod templates;
pub(crate) mod tenant_cache;

pub(crate) use state::AppState;

/// Build the full application router.
///
/// `master_conn_str` points at the global master.db (tenant registry).
/// `tenant_conn_str` points at the active tenant's SQLite file.
/// Phase 4 will resolve the tenant dynamically from JWT claims;
/// for now the single tenant is hard-coded at startup.
pub fn build_router(
    master_conn_str: &str,
    tenant_conn_str: &str,
    public_dir: &str,
    mailer: std::sync::Arc<dyn mailer::Mailer>,
) -> anyhow::Result<Router> {
    let state = AppState::new(master_conn_str, tenant_conn_str, mailer)?;

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

    Ok(handlers::create_router(state)
        .fallback_service(ServeDir::new(public_dir))
        .layer(axum::middleware::from_fn(request_id::request_tracing)))
}
