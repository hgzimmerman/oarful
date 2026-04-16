//! Lazy per-tenant connection pool cache. Opens tenant SQLite files
//! on demand (which runs migrations) and caches the `Db` handle for
//! subsequent requests. The cache is global across all requests —
//! `Mutex` is fine because the critical section is a HashMap
//! lookup/insert, never held across an await.

use std::collections::HashMap;
use std::sync::Mutex;

use lineup_db::state::Db;
use lineup_master_db::state::MasterDb;
use lineup_master_db::tenant::{BillingStatus, Tenant, TenantId};

/// Tenant-level UI configuration flags, cached from the master DB.
#[derive(Clone, Debug)]
pub(crate) struct TenantConfig {
    pub(crate) attributes_public: bool,
    pub(crate) force_cox_stern: bool,
    pub(crate) emails_visible: bool,
    pub(crate) tenant_name: String,
    pub(crate) tenant_slug: String,
    pub(crate) billing_status: BillingStatus,
    pub(crate) trial_expires_at: Option<chrono::NaiveDateTime>,
    pub(crate) is_demo: bool,
}

impl TenantConfig {
    pub(crate) fn from_tenant(t: &Tenant) -> Self {
        Self {
            attributes_public: t.are_attributes_public(),
            force_cox_stern: t.force_cox_stern(),
            emails_visible: t.are_emails_visible(),
            tenant_name: t.name.clone(),
            tenant_slug: t.slug.clone(),
            billing_status: t.billing_status(),
            trial_expires_at: t.trial_expires_at,
            is_demo: t.is_demo(),
        }
    }

    /// Whether this tenant has an active billing relationship.
    pub(crate) fn is_billing_ok(&self) -> bool {
        if self.is_demo {
            return true;
        }
        match self.billing_status {
            BillingStatus::Active | BillingStatus::Grandfathered => true,
            BillingStatus::Trial => self
                .trial_expires_at
                .map(|exp| exp > chrono::Utc::now().naive_utc())
                .unwrap_or(true),
            BillingStatus::Suspended | BillingStatus::Cancelled => false,
        }
    }
}

#[derive(Clone)]
struct CachedTenant {
    db: Db,
    config: TenantConfig,
}

pub(crate) struct TenantCache {
    tenants: Mutex<HashMap<TenantId, CachedTenant>>,
}

impl TenantCache {
    pub(crate) fn new() -> Self {
        Self {
            tenants: Mutex::new(HashMap::new()),
        }
    }

    /// Pre-warm the cache with a known tenant. Called at startup for
    /// the default tenant so its first request doesn't pay the
    /// migration cost.
    pub(crate) fn insert(&self, tenant_id: TenantId, db: Db, config: TenantConfig) {
        self.tenants
            .lock()
            .unwrap()
            .insert(tenant_id, CachedTenant { db, config });
    }

    /// Evict a tenant from the cache (e.g. after deleting a demo tenant).
    pub(crate) fn remove(&self, tenant_id: TenantId) {
        self.tenants.lock().unwrap().remove(&tenant_id);
    }

    /// Get the Db and config for a tenant, opening it on first access.
    /// Returns an error if the tenant doesn't exist in the master DB or
    /// the SQLite file can't be opened.
    pub(crate) async fn get_or_connect(
        &self,
        tenant_id: TenantId,
        master_db: &MasterDb,
    ) -> anyhow::Result<(Db, TenantConfig)> {
        // Fast path: already cached.
        if let Some(cached) = self.tenants.lock().unwrap().get(&tenant_id) {
            return Ok((cached.db.clone(), cached.config.clone()));
        }

        // Slow path: look up the tenant's db_path in the master DB
        // and open a new pool.
        let tenant = master_db
            .with_conn(move |conn| Tenant::get(conn, tenant_id))
            .await?;
        let tenant = tenant.ok_or_else(|| anyhow::anyhow!("unknown tenant {tenant_id}"))?;

        tracing::info!(
            tenant_id = %tenant_id,
            db_path = %tenant.db_path,
            "opening tenant database on first access"
        );
        let db = Db::connect(&tenant.db_path)?;
        let config = TenantConfig::from_tenant(&tenant);

        // Insert into cache. Another request may have raced us — that's
        // fine, the first writer wins and the second Db gets dropped.
        let cached = CachedTenant {
            db: db.clone(),
            config: config.clone(),
        };
        self.tenants
            .lock()
            .unwrap()
            .entry(tenant_id)
            .or_insert(cached);
        Ok((db, config))
    }
}
