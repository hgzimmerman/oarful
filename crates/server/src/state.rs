//! Shared application state passed to every handler as `State<AppState>`.
//!
//! Thin wrapper around [`lineup_db::state::Db`], which already owns the
//! deadpool-diesel pool and ran migrations on `connect`. Also carries a
//! [`tokio::sync::Semaphore`] that bounds concurrent solver invocations
//! across the whole process — see `handlers/solve.rs` for the rationale.

use std::sync::Arc;

use lineup_db::state::Db;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: Db,
    /// Bounds how many `lineup_solver::solve` calls can run at once.
    /// Cloned (cheap — it's an `Arc`) for each handler invocation.
    pub(crate) solve_semaphore: Arc<Semaphore>,
}

impl AppState {
    pub(crate) fn new(conn_str: &str) -> anyhow::Result<Self> {
        // Default: one solver slot per available CPU. Lets a handful
        // of coaches re-solve simultaneously without saturating the
        // tokio blocking pool that deadpool-diesel also lives on.
        // Override via the `SOLVE_CONCURRENCY` env var.
        let solve_concurrency = std::env::var("SOLVE_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|&n| n > 0)
            .or_else(|| {
                std::thread::available_parallelism()
                    .ok()
                    .map(|n| n.get())
            })
            .unwrap_or(2);
        tracing::info!(solve_concurrency, "configuring solver semaphore");
        Ok(Self {
            db: Db::connect(conn_str)?,
            solve_semaphore: Arc::new(Semaphore::new(solve_concurrency)),
        })
    }
}
