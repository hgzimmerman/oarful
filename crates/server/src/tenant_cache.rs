//! Lazy per-tenant connection pool cache. Opens tenant SQLite files
//! on demand (which runs migrations) and caches the `Db` handle for
//! subsequent requests. The cache is global across all requests —
//! `Mutex` is fine because the critical section is a HashMap
//! lookup/insert, never held across an await.

use std::collections::HashMap;
use std::sync::Mutex;

use lineup_db::state::Db;
use lineup_master_db::state::MasterDb;
use lineup_master_db::tenant::{Tenant, TenantId};

#[derive(Clone)]
struct CachedTenant {
    db: Db,
    attributes_public: bool,
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
    pub(crate) fn insert(&self, tenant_id: TenantId, db: Db, attributes_public: bool) {
        self.tenants.lock().unwrap().insert(
            tenant_id,
            CachedTenant {
                db,
                attributes_public,
            },
        );
    }

    /// Get the Db and config for a tenant, opening it on first access.
    /// Returns an error if the tenant doesn't exist in the master DB or
    /// the SQLite file can't be opened.
    pub(crate) async fn get_or_connect(
        &self,
        tenant_id: TenantId,
        master_db: &MasterDb,
    ) -> anyhow::Result<(Db, bool)> {
        // Fast path: already cached.
        if let Some(cached) = self.tenants.lock().unwrap().get(&tenant_id) {
            return Ok((cached.db.clone(), cached.attributes_public));
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
        let attributes_public = tenant.are_attributes_public();

        // Insert into cache. Another request may have raced us — that's
        // fine, the first writer wins and the second Db gets dropped.
        let cached = CachedTenant {
            db: db.clone(),
            attributes_public,
        };
        self.tenants
            .lock()
            .unwrap()
            .entry(tenant_id)
            .or_insert(cached);
        Ok((db, attributes_public))
    }
}
