//! Team selector and management. Currently just the navbar dropdown;
//! full team CRUD lands later with the Program Director admin views.

use axum::{http::StatusCode, response::Html, Extension};
use axum_extra::extract::CookieJar;
use lineup_db::team::Team;

use crate::{handlers::internal_error, state::TenantContext, templates};

/// `GET /teams/selector` — returns the team dropdown markup. Called
/// via `hx-trigger="load"` from the navbar placeholder so the layout
/// template stays a pure function.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn selector_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
) -> Result<Html<String>, StatusCode> {
    let active = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let teams = tenant
        .db
        .with_conn(|conn| Team::list_all(conn))
        .await
        .map_err(internal_error)?;
    Ok(Html(
        templates::teams::selector(&teams, active).into_string(),
    ))
}
