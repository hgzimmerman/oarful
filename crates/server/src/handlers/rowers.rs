//! `GET /rowers` — read-only roster view. Editing is deferred to a
//! future iteration per DESIGN.md.

use axum::{extract::State, http::StatusCode, response::Html};
use axum_htmx::HxRequest;
use lineup_db::rower::Rower;

use crate::{handlers::internal_error, state::AppState, templates};

pub async fn list_handler(
    State(state): State<AppState>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let rowers = state
        .db
        .with_conn(|conn| Rower::list_active(conn))
        .await
        .map_err(internal_error)?;
    let content = templates::rowers::list_content(&rowers);
    Ok(super::maybe_page("Rowers", content, hx))
}
