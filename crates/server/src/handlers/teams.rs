//! Team selector and management. Currently just the navbar dropdown;
//! full team CRUD lands later with the Program Director admin views.

use axum::{extract::State, http::StatusCode, response::Html};
use axum_extra::extract::CookieJar;
use lineup_db::team::Team;

use crate::{handlers::internal_error, state::AppState, templates};

/// `GET /teams/selector` — returns the team dropdown markup. Called
/// via `hx-trigger="load"` from the navbar placeholder so the layout
/// template stays a pure function.
pub(crate) async fn selector_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Html<String>, StatusCode> {
    let active = super::active_team(&state, &jar).await?;
    let teams = state
        .db
        .with_conn(|conn| Team::list_all(conn))
        .await
        .map_err(internal_error)?;
    Ok(Html(
        templates::teams::selector(&teams, active).into_string(),
    ))
}
