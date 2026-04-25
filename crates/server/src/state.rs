//! Shared application state passed to every handler as `State<AppState>`.
//!
//! Holds the master DB, a tenant cache for lazy per-tenant pool
//! opening, solver infrastructure, and JWT keys.
//!
//! Sub-states (`MailerCtx`, `SolverCtx`, `TenantDb`, `JwtKeys`) are
//! extractable via `FromRef<AppState>` so handlers declare only what
//! they need.

use std::sync::Arc;

use axum::extract::FromRef;
use lineup_db::state::Db;
use lineup_master_db::state::MasterDb;
use lineup_master_db::tenant::{BillingStatus, NewTenant, Tenant, TenantId};
use tokio::sync::Semaphore;

use crate::jwt::{Claims, JwtKeys};
use crate::mailer::Mailer;
use crate::tenant_cache::{TenantCache, TenantConfig};

#[derive(Clone)]
pub struct AppState {
    pub(crate) master_db: MasterDb,
    pub(crate) tenant_cache: Arc<TenantCache>,
    /// The default tenant's ID, used for backward compat and startup
    /// seeding. Phase 5+ may remove this once all access is JWT-driven.
    #[allow(dead_code)]
    pub(crate) default_tenant_id: TenantId,
    pub(crate) solve_semaphore: Arc<Semaphore>,
    pub(crate) solver_pool: Arc<rayon::ThreadPool>,
    pub(crate) jwt_keys: JwtKeys,
    pub(crate) mailer: Arc<dyn Mailer>,
    /// Base URL for generating full links (e.g. "http://localhost:3000").
    /// Read from the `ORIGIN` env var. None if not set — invite URLs
    /// will be relative paths.
    pub(crate) origin: Option<url::Url>,
    /// Base directory for tenant SQLite files. Default tenant lives at
    /// `{data_dir}/default.db`, demos at `{data_dir}/demos/{slug}.db`.
    pub(crate) data_dir: String,
    /// Email address of the global superuser (from `SUPERUSER_EMAIL`).
    pub(crate) superuser_email: Option<String>,
    /// Contact email for support/billing pages (from `WEBMASTER_EMAIL`).
    /// Falls back to `support@oarful.com` if not set.
    pub(crate) webmaster_email: String,
    /// When true, public signup is disabled. Existing accounts and demo
    /// still work. Set via `SIGNUP_DISABLED=1`.
    pub(crate) signup_disabled: bool,
}

impl AppState {
    /// Refresh all cached tenant configs from the master DB without
    /// dropping DB connections. Call after modifying tenant records
    /// (e.g. billing status) to make the server pick up the changes.
    pub async fn refresh_tenant_configs(&self) {
        self.tenant_cache.refresh_configs(&self.master_db).await;
    }
}

// ── Sub-states extractable via FromRef ──────────────────────────

/// Email delivery + full-URL generation.
#[derive(Clone)]
pub(crate) struct MailerCtx {
    pub(crate) mailer: Arc<dyn Mailer>,
    origin: Option<url::Url>,
}

impl MailerCtx {
    /// Build a full URL from a relative path. Returns the path as-is
    /// if ORIGIN is not configured.
    pub(crate) fn full_url(&self, path: &str) -> String {
        match &self.origin {
            Some(base) => {
                let mut u = base.clone();
                u.set_path(path);
                u.to_string()
            }
            None => path.to_string(),
        }
    }
}

impl FromRef<AppState> for MailerCtx {
    fn from_ref(state: &AppState) -> Self {
        Self {
            mailer: state.mailer.clone(),
            origin: state.origin.clone(),
        }
    }
}

/// Solver thread pool + concurrency semaphore.
#[derive(Clone)]
pub(crate) struct SolverCtx {
    pub(crate) solver_pool: Arc<rayon::ThreadPool>,
    pub(crate) solve_semaphore: Arc<Semaphore>,
}

impl FromRef<AppState> for SolverCtx {
    fn from_ref(state: &AppState) -> Self {
        Self {
            solver_pool: state.solver_pool.clone(),
            solve_semaphore: state.solve_semaphore.clone(),
        }
    }
}

/// Tenant resolution: master DB + connection cache + data directory.
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct TenantDb {
    pub(crate) master_db: MasterDb,
    pub(crate) tenant_cache: Arc<TenantCache>,
    pub(crate) data_dir: String,
}

#[allow(dead_code)]
impl TenantDb {
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

    /// Evict a tenant from the connection cache. The next request for
    /// this tenant will re-read its config from the master DB.
    pub fn evict_tenant(&self, tenant_id: TenantId) {
        self.tenant_cache.remove(tenant_id);
    }

    /// Resolve a tenant by slug. Looks up the tenant_id in the master
    /// DB, then delegates to [`tenant_db`].
    pub(crate) async fn tenant_db_by_slug(
        &self,
        slug: &str,
    ) -> anyhow::Result<(TenantId, Db, TenantConfig)> {
        let slug = slug.to_string();
        let tenant = self
            .master_db
            .with_conn(move |conn| Tenant::find_by_slug(conn, &slug))
            .await?;
        let tenant = tenant.ok_or_else(|| anyhow::anyhow!("unknown tenant slug"))?;
        let (db, config) = self.tenant_db(tenant.id).await?;
        Ok((tenant.id, db, config))
    }
}

impl FromRef<AppState> for TenantDb {
    fn from_ref(state: &AppState) -> Self {
        Self {
            master_db: state.master_db.clone(),
            tenant_cache: state.tenant_cache.clone(),
            data_dir: state.data_dir.clone(),
        }
    }
}

impl FromRef<AppState> for JwtKeys {
    fn from_ref(state: &AppState) -> Self {
        state.jwt_keys.clone()
    }
}

/// Bundle of per-request tenant state injected into request
/// extensions by the `require_auth` middleware. Handlers extract
/// this instead of reading `state.db` directly.
#[derive(Clone)]
pub(crate) struct TenantContext {
    pub(crate) db: Db,
    #[allow(dead_code)]
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
                .at_least(lineup_db::app_user::Role::Coach)
    }

    /// Whether the current user should see rower email addresses.
    /// True when the tenant has enabled email visibility or the user
    /// is Coach+.
    pub(crate) fn show_emails(&self) -> bool {
        self.config.emails_visible
            || self
                .claims
                .role()
                .at_least(lineup_db::app_user::Role::Coach)
    }
}

impl AppState {
    pub(crate) fn new(
        master_conn_str: &str,
        data_dir: &str,
        mailer: Arc<dyn Mailer>,
    ) -> anyhow::Result<Self> {
        let master_db = MasterDb::connect(master_conn_str)?;

        // Ensure data directories exist.
        std::fs::create_dir_all(format!("{data_dir}/demos"))
            .map_err(|e| anyhow::anyhow!("creating data dirs: {e}"))?;
        std::fs::create_dir_all(format!("{data_dir}/tenants"))
            .map_err(|e| anyhow::anyhow!("creating data dirs: {e}"))?;

        let default_db_path = format!("{data_dir}/default.db");
        let default_tenant_id = ensure_default_tenant(master_conn_str, &default_db_path)?;

        // Pre-warm the cache with the default tenant.
        let default_db = Db::connect(&default_db_path)?;
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
            .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
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
            tracing::warn!(
                "JWT_SECRET not set — using random secret (tokens won't survive restart)"
            );
            format!("dev-{h:x}")
        });
        let jwt_keys = JwtKeys::from_secret(jwt_secret.as_bytes());

        let origin = std::env::var("ORIGIN").ok().and_then(|s| {
            let s = s.trim().to_string();
            if s.is_empty() {
                return None;
            }
            match url::Url::parse(&s) {
                Ok(u) => {
                    tracing::info!(%u, "using ORIGIN for full URLs");
                    Some(u)
                }
                Err(e) => {
                    tracing::warn!(%s, %e, "ORIGIN is not a valid URL — ignoring");
                    None
                }
            }
        });
        if origin.is_none() {
            tracing::info!("ORIGIN not set — invite URLs will be relative paths");
        }

        let superuser_email = std::env::var("SUPERUSER_EMAIL")
            .ok()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
        if let Some(ref email) = superuser_email {
            tracing::info!(%email, "superuser configured");
        }

        let webmaster_email = std::env::var("WEBMASTER_EMAIL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "support@oarful.com".to_string());

        let signup_disabled = std::env::var("SIGNUP_DISABLED")
            .ok()
            .map(|s| matches!(s.trim(), "1" | "true" | "yes"))
            .unwrap_or(false);
        if signup_disabled {
            tracing::info!("public signup is disabled");
        }

        Ok(Self {
            master_db,
            tenant_cache,
            default_tenant_id,
            solve_semaphore: Arc::new(Semaphore::new(solve_concurrency)),
            solver_pool: Arc::new(solver_pool),
            jwt_keys,
            mailer,
            origin,
            data_dir: data_dir.to_string(),
            superuser_email,
            webmaster_email,
            signup_disabled,
        })
    }

    // ── Delegation to sub-states ───────────────────────────────
    // Used by the auth middleware and demo cleanup, which operate
    // on the full AppState.

    pub(crate) async fn tenant_db(
        &self,
        tenant_id: TenantId,
    ) -> anyhow::Result<(Db, TenantConfig)> {
        self.tenant_cache
            .get_or_connect(tenant_id, &self.master_db)
            .await
    }

    pub(crate) fn evict_tenant(&self, tenant_id: TenantId) {
        self.tenant_cache.remove(tenant_id);
    }

    pub(crate) async fn tenant_db_by_slug(
        &self,
        slug: &str,
    ) -> anyhow::Result<(TenantId, Db, TenantConfig)> {
        let slug = slug.to_string();
        let tenant = self
            .master_db
            .with_conn(move |conn| Tenant::find_by_slug(conn, &slug))
            .await?;
        let tenant = tenant.ok_or_else(|| anyhow::anyhow!("unknown tenant slug"))?;
        let (db, config) = self.tenant_db(tenant.id).await?;
        Ok((tenant.id, db, config))
    }

    pub(crate) fn full_url(&self, path: &str) -> String {
        MailerCtx::from_ref(self).full_url(path)
    }

    pub(crate) fn is_superuser_email(&self, email: &str) -> bool {
        self.superuser_email.as_ref().is_some_and(|su| su == email)
    }
}

fn lookup_tenant(master_conn_str: &str, id: TenantId) -> anyhow::Result<Tenant> {
    use anyhow::Context;
    let mut conn = lineup_master_db::connect_sync(master_conn_str)
        .context("opening master DB for tenant lookup")?;
    Tenant::get(&mut conn, id)?.ok_or_else(|| anyhow::anyhow!("tenant {id} not found"))
}

fn ensure_default_tenant(master_conn_str: &str, tenant_db_path: &str) -> anyhow::Result<TenantId> {
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
            billing_status: BillingStatus::Active,
        },
    )?;
    tracing::info!(tenant_id = %tenant.id, "Created default tenant");
    Ok(tenant.id)
}
