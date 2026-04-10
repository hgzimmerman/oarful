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

pub(crate) struct TenantCache {
    dbs: Mutex<HashMap<TenantId, Db>>,
}

impl TenantCache {
    pub(crate) fn new() -> Self {
        Self {
            dbs: Mutex::new(HashMap::new()),
        }
    }

    /// Pre-warm the cache with a known tenant. Called at startup for
    /// the default tenant so its first request doesn't pay the
    /// migration cost.
    pub(crate) fn insert(&self, tenant_id: TenantId, db: Db) {
        self.dbs.lock().unwrap().insert(tenant_id, db);
    }

    /// Get the Db for a tenant, opening it on first access. Returns
    /// an error if the tenant doesn't exist in the master DB or the
    /// SQLite file can't be opened.
    pub(crate) async fn get_or_connect(
        &self,
        tenant_id: TenantId,
        master_db: &MasterDb,
    ) -> anyhow::Result<Db> {
        // Fast path: already cached.
        if let Some(db) = self.dbs.lock().unwrap().get(&tenant_id) {
            return Ok(db.clone());
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

        // Insert into cache. Another request may have raced us — that's
        // fine, the first writer wins and the second Db gets dropped.
        self.dbs.lock().unwrap().entry(tenant_id).or_insert(db.clone());
        Ok(db)
    }
}
