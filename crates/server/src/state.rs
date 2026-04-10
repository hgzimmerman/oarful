//! Shared application state passed to every handler as `State<AppState>`.
//!
//! Holds the master DB (tenant registry), the active tenant's DB pool,
//! a solver semaphore, and a dedicated rayon thread pool for solver work.
//!
//! Phase 2 hard-codes a single tenant. Phase 4 will add a
//! `DashMap<TenantId, Db>` for dynamic multi-tenant resolution from
//! JWT claims.

use std::sync::Arc;

use lineup_db::state::Db;
use lineup_master_db::state::MasterDb;
use lineup_master_db::tenant::{NewTenant, Tenant, TenantId};
use tokio::sync::Semaphore;

use crate::jwt::JwtKeys;

#[derive(Clone)]
pub(crate) struct AppState {
    /// Global tenant registry. Tiny, rarely queried.
    pub(crate) master_db: MasterDb,
    /// The single active tenant's DB pool. Phase 4 replaces this with
    /// a per-tenant cache keyed by `TenantId`.
    pub(crate) db: Db,
    /// The tenant ID for the hard-coded single tenant.
    pub(crate) tenant_id: TenantId,
    /// Bounds how many `lineup_solver::solve` calls can run at once.
    pub(crate) solve_semaphore: Arc<Semaphore>,
    /// Dedicated CPU pool for solver work, isolated from tokio's
    /// blocking pool.
    pub(crate) solver_pool: Arc<rayon::ThreadPool>,
    /// JWT signing/verification keys.
    pub(crate) jwt_keys: JwtKeys,
}

impl AppState {
    /// Boot the server state. Opens (or creates) the master DB, ensures
    /// a default tenant exists pointing at `tenant_conn_str`, then opens
    /// the tenant DB pool.
    pub(crate) fn new(
        master_conn_str: &str,
        tenant_conn_str: &str,
    ) -> anyhow::Result<Self> {
        let master_db = MasterDb::connect(master_conn_str)?;

        // Ensure the default tenant exists in the master registry.
        // Use a one-shot sync connection (MasterDb::connect already
        // ran migrations). Calling `with_conn` requires a tokio
        // runtime which may not be set up at this point.
        let tenant_id = ensure_default_tenant(master_conn_str, tenant_conn_str)?;

        let db = Db::connect(tenant_conn_str)?;

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

        // JWT secret: from env or random (dev-mode; tokens don't
        // survive restarts).
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
            use std::collections::hash_map::RandomState;
            use std::hash::{BuildHasher, Hasher};
            let s = RandomState::new();
            let h = s.build_hasher().finish();
            tracing::warn!("JWT_SECRET not set — using random secret (tokens won't survive restart)");
            format!("dev-{h:x}")
        });
        let jwt_keys = JwtKeys::from_secret(jwt_secret.as_bytes());

        Ok(Self {
            master_db,
            db,
            tenant_id,
            solve_semaphore: Arc::new(Semaphore::new(solve_concurrency)),
            solver_pool: Arc::new(solver_pool),
            jwt_keys,
        })
    }
}

/// Seed a "default" tenant row if one doesn't already exist. Returns
/// its `TenantId`. Uses a one-shot sync connection since this runs
/// at startup before the async runtime is fully available.
fn ensure_default_tenant(
    master_conn_str: &str,
    tenant_db_path: &str,
) -> anyhow::Result<TenantId> {
    use anyhow::Context;
    let mut conn = lineup_master_db::connect_sync(master_conn_str)
        .context("opening master DB for tenant seed")?;
    if let Some(existing) = Tenant::find_by_slug(&mut conn, "default")? {
        tracing::info!(tenant_id = %existing.id, "Using existing default tenant");
        return Ok(existing.id);
    }
    let now = chrono::Utc::now().naive_utc();
    let tenant = Tenant::create(
        &mut conn,
        NewTenant {
            name: "Default Club".to_string(),
            slug: "default".to_string(),
            db_path: tenant_db_path.to_string(),
            created_at: now,
        },
    )?;
    tracing::info!(tenant_id = %tenant.id, "Created default tenant");
    Ok(tenant.id)
}
