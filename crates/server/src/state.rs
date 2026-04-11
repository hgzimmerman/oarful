//! Shared application state passed to every handler as `State<AppState>`.
//!
//! Holds the master DB, a tenant cache for lazy per-tenant pool
//! opening, solver infrastructure, and JWT keys.

use std::sync::Arc;

use lineup_db::state::Db;
use lineup_master_db::state::MasterDb;
use lineup_master_db::tenant::{NewTenant, Tenant, TenantId};
use tokio::sync::Semaphore;

use crate::jwt::{Claims, JwtKeys};
use crate::mailer::Mailer;
use crate::tenant_cache::{TenantCache, TenantConfig};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) master_db: MasterDb,
    pub(crate) tenant_cache: Arc<TenantCache>,
    /// The default tenant's ID, used for backward compat and startup
    /// seeding. Phase 5+ may remove this once all access is JWT-driven.
    pub(crate) default_tenant_id: TenantId,
    pub(crate) solve_semaphore: Arc<Semaphore>,
    pub(crate) solver_pool: Arc<rayon::ThreadPool>,
    pub(crate) jwt_keys: JwtKeys,
    pub(crate) mailer: Arc<dyn Mailer>,
}

/// Bundle of per-request tenant state injected into request
/// extensions by the `require_auth` middleware. Handlers extract
/// this instead of reading `state.db` directly.
#[derive(Clone)]
pub(crate) struct TenantContext {
    pub(crate) db: Db,
    pub(crate) tenant_id: TenantId,
    pub(crate) claims: Claims,
    pub(crate) config: TenantConfig,
}

impl TenantContext {
    /// Whether the current user should see rower attributes (weight
    /// class, skill, strength). True when the tenant is transparent
    /// or the user is Coach+.
    pub(crate) fn show_attributes(&self) -> bool {
        self.config.attributes_public
            || self
                .claims
                .role()
                .unwrap_or(lineup_db::app_user::Role::Member)
                .at_least(lineup_db::app_user::Role::Coach)
    }
}

impl AppState {
    pub(crate) fn new(
        master_conn_str: &str,
        tenant_conn_str: &str,
        mailer: Arc<dyn Mailer>,
    ) -> anyhow::Result<Self> {
        let master_db = MasterDb::connect(master_conn_str)?;

        let default_tenant_id = ensure_default_tenant(master_conn_str, tenant_conn_str)?;

        // Pre-warm the cache with the default tenant.
        let default_db = Db::connect(tenant_conn_str)?;
        let default_tenant = lookup_tenant(master_conn_str, default_tenant_id)?;
        let tenant_cache = Arc::new(TenantCache::new());
        tenant_cache.insert(
            default_tenant_id,
            default_db,
            TenantConfig::from_tenant(&default_tenant),
        );

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
            tenant_cache,
            default_tenant_id,
            solve_semaphore: Arc::new(Semaphore::new(solve_concurrency)),
            solver_pool: Arc::new(solver_pool),
            jwt_keys,
            mailer,
        })
    }

    /// Resolve a tenant's Db and config from the cache, opening it on
    /// first access. Used by the auth middleware and public invite routes.
    pub(crate) async fn tenant_db(
        &self,
        tenant_id: TenantId,
    ) -> anyhow::Result<(Db, TenantConfig)> {
        self.tenant_cache
            .get_or_connect(tenant_id, &self.master_db)
            .await
    }
}

fn lookup_tenant(master_conn_str: &str, id: TenantId) -> anyhow::Result<Tenant> {
    use anyhow::Context;
    let mut conn = lineup_master_db::connect_sync(master_conn_str)
        .context("opening master DB for tenant lookup")?;
    Tenant::get(&mut conn, id)?
        .ok_or_else(|| anyhow::anyhow!("tenant {id} not found"))
}

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
