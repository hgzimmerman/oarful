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
/// The `public_dir` path is resolved relative to the process's current
/// working directory; in dev that's typically the workspace root, so
/// passing `"crates/server/public"` works out of the box. Production
/// deployments should pass an absolute path next to the executable.
pub fn build_router(conn_string: &str, public_dir: &str) -> anyhow::Result<Router> {
    let state = AppState::new(conn_string)?;
    Ok(handlers::create_router()
        .fallback_service(ServeDir::new(public_dir))
        .with_state(state)
        .layer(TraceLayer::new_for_http()))
}
