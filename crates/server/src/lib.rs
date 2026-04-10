//! lineup_server — coach-facing HTTP UI for the lineup generator.
//!
//! A thin axum web server that wraps [`lineup_solver::solve`] and
//! [`lineup_db`] in a server-rendered Maud + HTMX frontend. See
//! `DESIGN.md` in this crate for the architecture rationale.

use axum::Router;
use tower_http::{services::ServeDir, trace::TraceLayer};

pub(crate) mod handlers;
pub(crate) mod state;
pub(crate) mod templates;

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
) -> anyhow::Result<Router> {
    let state = AppState::new(master_conn_str, tenant_conn_str)?;
    Ok(handlers::create_router()
        .fallback_service(ServeDir::new(public_dir))
        .with_state(state)
        .layer(TraceLayer::new_for_http()))
}
