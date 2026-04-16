//! Test helpers shared across every crate that wants to exercise the
//! database layer. Unconditionally exported (rather than feature-gated
//! behind `test-support`) because this crate is the workspace's
//! schema authority and its consumers regularly want a "give me a
//! scratch in-memory db" hook in their own test modules.
//!
//! The entry points are intentionally minimal — callers seed whatever
//! fixture data their test needs on top.

use diesel::{Connection, SqliteConnection};
use diesel_migrations::MigrationHarness;

use crate::MIGRATIONS;

/// Build a fresh in-memory sqlite connection with the embedded
/// migrations already applied. Each call returns an independent,
/// empty database — there is no cross-test state leak.
///
/// Panics on failure because tests should be the only caller and a
/// broken migration / connect step is something the harness should
/// surface loudly rather than quietly.
pub fn in_memory_conn() -> SqliteConnection {
    let mut conn =
        SqliteConnection::establish(":memory:").expect("establishing in-memory sqlite for tests");
    conn.run_pending_migrations(MIGRATIONS)
        .expect("running embedded migrations against in-memory sqlite");
    conn
}
