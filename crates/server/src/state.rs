//! Shared application state passed to every handler as `State<AppState>`.
//!
//! Thin wrapper around [`lineup_db::state::Db`], which already owns the
//! deadpool-diesel pool and ran migrations on `connect`. Keeping a fresh
//! struct here (rather than using `Db` directly as the state) leaves room
//! for additional fields later — e.g. a tera cache, a cookie key, a
//! solver-job queue — without a churny refactor across every handler.

use lineup_db::state::Db;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: Db,
}

impl AppState {
    pub(crate) fn new(conn_str: &str) -> anyhow::Result<Self> {
        Ok(Self {
            db: Db::connect(conn_str)?,
        })
    }
}
