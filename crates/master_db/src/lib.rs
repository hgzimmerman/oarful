//! Master database layer for the lineup generator multi-tenant system.
//!
//! The master DB is a tiny global SQLite file that tracks which tenant
//! DBs exist and where they live on disk. Each tenant (rowing club) has
//! its own separate SQLite file containing the full domain schema. The
//! master DB never stores rowing data — just the registry.

pub mod schema;
pub mod state;
pub mod tenant;

use diesel_migrations::{embed_migrations, EmbeddedMigrations};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

/// Open a bare synchronous connection to the master DB. Used at
/// server startup (before the async pool is built) for one-shot
/// seeding operations.
pub fn connect_sync(conn_str: &str) -> Result<diesel::SqliteConnection, diesel::ConnectionError> {
    use diesel::Connection;
    diesel::SqliteConnection::establish(conn_str)
}
