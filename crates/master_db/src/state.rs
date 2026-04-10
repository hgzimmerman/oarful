//! Connection pool wrapper for the master database — mirrors
//! [`lineup_db::state::Db`] but for the global tenant registry.

use anyhow::Context;
use deadpool_diesel::sqlite::{Manager, Object, Pool};
use deadpool_diesel::Runtime;
use diesel::{Connection, SqliteConnection};
use diesel_migrations::MigrationHarness;
use std::sync::Arc;

use crate::MIGRATIONS;

#[derive(Clone)]
pub struct MasterDb {
    pool: Arc<Pool>,
}

impl std::fmt::Debug for MasterDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MasterDb").finish()
    }
}

impl MasterDb {
    pub fn connect(conn_str: &str) -> anyhow::Result<Self> {
        tracing::info!(conn_str, "Opening master database...");
        let mut conn = SqliteConnection::establish(conn_str)
            .with_context(|| format!("establishing sync conn for master migrations: {conn_str}"))?;
        let applied = conn
            .run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("running master migrations: {e}"))?;
        if applied.is_empty() {
            tracing::info!("Master database is up to date");
        } else {
            tracing::info!(count = applied.len(), "Applied master migrations");
        }
        drop(conn);

        let manager = Manager::new(conn_str, Runtime::Tokio1);
        let pool = Pool::builder(manager)
            .max_size(4) // master DB is tiny, rarely queried
            .build()
            .context("building master deadpool")?;

        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    pub async fn with_conn<T, F>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error> + Send + 'static,
        T: Send + 'static,
    {
        let obj: Object = self
            .pool
            .get()
            .await
            .context("acquiring master pool connection")?;
        obj.interact(f)
            .await
            .map_err(|e| anyhow::anyhow!("master pool interact panic: {e}"))?
            .context("master database query")
    }
}
