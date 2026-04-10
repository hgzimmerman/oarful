//! Shared application state passed to every handler as `State<AppState>`.
//!
//! Carries the DB pool, a concurrency semaphore, and a dedicated rayon
//! thread pool for solver work. The rayon pool keeps `solve()` CPU
//! time off tokio's blocking pool so deadpool-diesel DB queries aren't
//! starved under concurrent load.

use std::sync::Arc;

use lineup_db::state::Db;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: Db,
    /// Bounds how many `lineup_solver::solve` calls can run at once.
    pub(crate) solve_semaphore: Arc<Semaphore>,
    /// Dedicated CPU pool for solver work, isolated from tokio's
    /// blocking pool. Sized to `solve_concurrency` threads.
    pub(crate) solver_pool: Arc<rayon::ThreadPool>,
}

impl AppState {
    pub(crate) fn new(conn_str: &str) -> anyhow::Result<Self> {
        // Default: one solver slot per available CPU. Override via
        // the `SOLVE_CONCURRENCY` env var.
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
        tracing::info!(solve_concurrency, "configuring solver pool + semaphore");

        let solver_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(solve_concurrency)
            .thread_name(|i| format!("solver-{i}"))
            .build()
            .map_err(|e| anyhow::anyhow!("building solver thread pool: {e}"))?;

        Ok(Self {
            db: Db::connect(conn_str)?,
            solve_semaphore: Arc::new(Semaphore::new(solve_concurrency)),
            solver_pool: Arc::new(solver_pool),
        })
    }
}
