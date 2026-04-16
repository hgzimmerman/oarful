//! Connection-pool wrapper.
//!
//! The blessed way to run queries against the database from async code is
//! [`Db::with_conn`], which takes a closure over `&mut SqliteConnection` and
//! collapses the `pool.get().await?.interact(|c| ...).await??` triple-result
//! into a single `anyhow::Result<T>`.

use anyhow::Context;
use deadpool_diesel::sqlite::{Manager, Object, Pool};
use deadpool_diesel::Runtime;
use diesel::{Connection, RunQueryDsl, SqliteConnection};
use diesel_migrations::MigrationHarness;
use std::sync::Arc;

use crate::MIGRATIONS;

/// Per-file PRAGMAs. These persist across connections so we only need
/// to set them once per database file (on the migration connection).
const FILE_PRAGMAS: &str = "\
    PRAGMA journal_mode = WAL;\
    PRAGMA foreign_keys = ON;\
";

/// Per-connection PRAGMAs. These reset on each new connection so we
/// apply them on every pool checkout.
const CONN_PRAGMAS: &str = "\
    PRAGMA busy_timeout = 5000;\
    PRAGMA synchronous = NORMAL;\
    PRAGMA cache_size = -8000;\
    PRAGMA mmap_size = 134217728;\
    PRAGMA foreign_keys = ON;\
";

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

        // Set file-level PRAGMAs before migrations (WAL mode makes
        // migrations faster and is required for Litestream backups).
        diesel::sql_query(FILE_PRAGMAS)
            .execute(&mut conn)
            .with_context(|| "setting file PRAGMAs")?;

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
            .max_size(16)
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
    ///   2. applies per-connection PRAGMAs (busy_timeout, synchronous, etc.),
    ///   3. runs the closure on the pool's blocking runtime via `interact`,
    ///   4. flattens the three error layers (pool, interact panic, diesel)
    ///      into one `anyhow::Error`.
    ///
    /// Wrap multi-statement work in `conn.transaction(|c| ...)` inside the
    /// closure when you need atomicity.
    pub async fn with_conn<T, F>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error> + Send + 'static,
        T: Send + 'static,
    {
        let obj: Object = self.pool.get().await.context("acquiring pool connection")?;
        obj.interact(move |conn| {
            // Per-connection PRAGMAs. These are cheap (no I/O) and
            // idempotent, so re-applying on every checkout is fine.
            // The alternative (a post_create hook) isn't available
            // in deadpool-diesel.
            let _ = diesel::sql_query(CONN_PRAGMAS).execute(conn);
            f(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("pool interact panic: {e}"))?
        .context("database query")
    }
}
