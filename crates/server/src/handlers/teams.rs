//! Team selector and management.

use axum::{
    extract::Path,
    http::StatusCode,
    response::Html,
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use lineup_db::app_user::Role;
use lineup_db::team::{Team, TeamId};
use serde::Deserialize;

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

#[derive(Debug, Deserialize)]
pub(crate) struct CreateTeamInput {
    name: String,
}

/// `POST /teams` — create a new team (PD only).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn create_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<CreateTeamInput>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let now = chrono::Utc::now().naive_utc();
    let team = tenant
        .db
        .with_conn(move |conn| {
            Team::create(conn, lineup_db::team::NewTeam { name, created_at: now })
        })
        .await
        .map_err(internal_error)?;

    // Redirect to the new team's detail page.
    let content = templates::teams::detail_content(&team);
    Ok(super::maybe_page_authed(&format!("Team · {}", team.name), content, hx, &tenant))
}

/// `GET /teams` — list all teams (PD only).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn list_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let teams = tenant
        .db
        .with_conn(|conn| Team::list_all(conn))
        .await
        .map_err(internal_error)?;
    let content = templates::teams::list_content(&teams);
    Ok(super::maybe_page_authed("Teams", content, hx, &tenant))
}

/// `GET /teams/{id}` — team detail + config (PD only).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn detail_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<TeamId>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let team = tenant
        .db
        .with_conn(move |conn| Team::get(conn, id))
        .await
        .map_err(internal_error)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let content = templates::teams::detail_content(&team);
    Ok(super::maybe_page_authed(&format!("Team · {}", team.name), content, hx, &tenant))
}

#[derive(Debug, Deserialize)]
pub(crate) struct TeamUpdateInput {
    name: String,
    self_edit_level: String,
}

/// `POST /teams/{id}` — update team config (PD only).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn update_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<TeamId>,
    hx: HxRequest,
    Form(input): Form<TeamUpdateInput>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let name = input.name.trim().to_string();
    let level = input.self_edit_level.clone();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    tenant
        .db
        .with_conn(move |conn| {
            use diesel::prelude::*;
            use lineup_db::schema::team;
            diesel::update(team::table.find(id))
                .set((
                    team::name.eq(&name),
                    team::self_edit_level.eq(&level),
                ))
                .execute(conn)
        })
        .await
        .map_err(internal_error)?;

    // Re-load and re-render.
    let team = tenant
        .db
        .with_conn(move |conn| Team::get(conn, id))
        .await
        .map_err(internal_error)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let content = templates::teams::detail_content(&team);
    Ok(super::maybe_page_authed(&format!("Team · {}", team.name), content, hx, &tenant))
}
