//! Connection-pool wrapper.
//!
//! The blessed way to run queries against the database from async code is
//! [`Db::with_conn`], which takes a closure over `&mut SqliteConnection` and
//! collapses the `pool.get().await?.interact(|c| ...).await??` triple-result
//! into a single `anyhow::Result<T>`.

use anyhow::Context;
use deadpool_diesel::sqlite::{Manager, Object, Pool};
use deadpool_diesel::Runtime;
use diesel::{Connection, SqliteConnection};
use diesel_migrations::MigrationHarness;
use std::sync::Arc;

use crate::MIGRATIONS;

#[derive(Clone)]
pub struct Db {
    pool: Arc<Pool>,
}

impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db").finish()
    }
}

impl Db {
    /// Opens the sqlite file at `conn_str`, runs pending migrations against a
    /// one-off sync connection, then builds a deadpool pool on top.
    ///
    /// Migrations run against a direct `SqliteConnection` rather than a pooled
    /// one because `diesel_migrations` is synchronous and we want the startup
    /// guarantee that schema is ready before any handler can borrow a conn.
    pub fn connect(conn_str: &str) -> anyhow::Result<Self> {
        tracing::info!(conn_str, "Checking for pending database migrations...");
        let mut conn = SqliteConnection::establish(conn_str)
            .with_context(|| format!("establishing sync conn for migrations: {conn_str}"))?;
        let applied = conn
            .run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("running migrations: {e}"))?;
        if applied.is_empty() {
            tracing::info!("Database is up to date, no migrations needed");
        } else {
            tracing::info!(count = applied.len(), "Applied migrations");
        }
        drop(conn);

        let manager = Manager::new(conn_str, Runtime::Tokio1);
        let pool = Pool::builder(manager)
            .max_size(40)
            .build()
            .context("building deadpool-diesel pool")?;

        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    pub fn pool(&self) -> Arc<Pool> {
        self.pool.clone()
    }

    /// Run a fallible closure against a pooled sync sqlite connection.
    ///
    /// This is the primary data-access entry point for async code. It:
    ///   1. checks out a connection from the pool,
    ///   2. runs the closure on the pool's blocking runtime via `interact`,
    ///   3. flattens the three error layers (pool, interact panic, diesel)
    ///      into one `anyhow::Error`.
    ///
    /// Wrap multi-statement work in `conn.transaction(|c| ...)` inside the
    /// closure when you need atomicity.
    pub async fn with_conn<T, F>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error> + Send + 'static,
        T: Send + 'static,
    {
        let obj: Object = self
            .pool
            .get()
            .await
            .context("acquiring pool connection")?;
        obj.interact(f)
            .await
            .map_err(|e| anyhow::anyhow!("pool interact panic: {e}"))?
            .context("database query")
    }
}
