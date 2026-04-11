//! HTTP handlers. Each submodule owns its own routes and templates;
//! [`create_router`] wires them all together.

use axum::{
    http::StatusCode,
    response::{Html, Redirect},
    routing::{get, post},
    Form, Router,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use lineup_db::team::TeamId;
use maud::Markup;
use serde::Deserialize;

use crate::{state::AppState, templates};

pub(crate) mod auth;
pub(crate) mod boats;
pub(crate) mod my;
pub(crate) mod users;
pub(crate) mod history;
pub(crate) mod practices;
pub(crate) mod rowers;
pub(crate) mod solve;
pub(crate) mod sync;
pub(crate) mod teams;

/// Compose the full route table. Called from [`crate::build_router`] so
/// the binary doesn't need to know about individual handlers.
pub(crate) fn create_router(state: AppState) -> Router {
    // Public routes — no auth required.
    let public = Router::new()
        .route("/login", get(auth::login_page).post(auth::login_handler))
        .route("/logout", post(auth::logout_handler))
        .route(
            "/invite/{token}",
            get(users::accept_page).post(users::accept_handler),
        )
        .with_state(state.clone());

    // Protected routes — require a valid JWT cookie.
    let protected = Router::new()
        .route("/", get(|| async { Redirect::permanent("/practices") }))
        .route("/practices", get(practices::list_handler).post(practices::create_handler))
        .route("/solve/{date}", get(solve::view_handler))
        .route("/commit/{date}", post(solve::commit_handler))
        .route("/commit-lineup/{date}", post(solve::commit_lineup_handler))
        .route("/history", get(history::list_handler))
        .route("/history/{date}", get(history::detail_handler))
        .route("/history/{date}/notes", post(history::notes_handler))
        .route(
            "/boats",
            get(boats::list_handler).post(boats::create_handler),
        )
        .route("/boats/new", get(boats::new_handler))
        .route(
            "/boats/{id}",
            get(boats::edit_handler)
                .put(boats::update_handler)
                .post(boats::update_handler),
        )
        .route("/rowers", get(rowers::list_handler))
        .route(
            "/rowers/{id}",
            get(rowers::detail_handler).post(rowers::update_handler),
        )
        .route("/rowers/{id}/attributes", get(rowers::attributes_handler))
        .route("/rowers/{id}/edit-attributes", get(rowers::edit_attributes_handler))
        .route(
            "/rowers/{id}/seat-affinity",
            post(rowers::seat_affinity_upsert_handler),
        )
        .route(
            "/rowers/{id}/seat-affinity/delete",
            post(rowers::seat_affinity_delete_handler),
        )
        .route(
            "/rowers/{id}/pair-affinity",
            post(rowers::pair_affinity_upsert_handler),
        )
        .route(
            "/rowers/{id}/pair-affinity/delete",
            post(rowers::pair_affinity_delete_handler),
        )
        .route("/sync", get(sync::form_handler).post(sync::sync_handler))
        .route("/users", get(users::list_handler))
        .route("/users/invite", post(users::invite_handler))
        .route(
            "/users/{id}/resend-invite",
            post(users::resend_invite_handler),
        )
        .route(
            "/my/profile",
            get(my::profile_handler).post(my::profile_update_handler),
        )
        .route(
            "/my/availability",
            get(my::availability_handler).post(my::availability_update_handler),
        )
        .route("/teams/selector", get(teams::selector_handler))
        .route("/switch-team", post(switch_team_handler))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .with_state(state);

    public.merge(protected)
}

/// Render `content` either as a full page (for a normal navigation) or
/// as the bare inner content (when HTMX is doing an in-place swap).
/// Every visual handler should return through here so the HTMX and
/// non-HTMX paths stay in sync.
pub(crate) fn maybe_page(
    title: &str,
    content: Markup,
    HxRequest(is_htmx): HxRequest,
) -> Html<String> {
    if is_htmx {
        Html(content.into_string())
    } else {
        Html(templates::layout::page(title, content).into_string())
    }
}

/// Collapse an anyhow/diesel/etc. error into a 500 response and log it.
/// Handlers use `.map_err(internal_error)` as their escape hatch.
pub(crate) fn internal_error<E: std::fmt::Debug>(error: E) -> StatusCode {
    tracing::error!(?error, "handler error");
    StatusCode::INTERNAL_SERVER_ERROR
}

// =====================================================================
// Active-team context (cookie-based until JWT lands in Phase 3)
// =====================================================================

/// Extract the active team from the TenantContext's JWT claims.
/// Falls back to the `active_team_id` cookie then the first team
/// in the DB.
pub(crate) async fn active_team(
    db: &lineup_db::state::Db,
    jar: &CookieJar,
    claims: Option<&crate::jwt::Claims>,
) -> Result<TeamId, StatusCode> {
    if let Some(c) = claims {
        return Ok(c.team_id());
    }
    if let Some(cookie) = jar.get("active_team_id") {
        if let Ok(id) = cookie.value().parse::<TeamId>() {
            return Ok(id);
        }
    }
    let team = db
        .with_conn(|conn| lineup_db::team::Team::first(conn))
        .await
        .map_err(internal_error)?;
    team.map(|t| t.id).ok_or_else(|| {
        tracing::error!("no teams in the database");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[derive(Debug, Deserialize)]
pub(crate) struct TeamSwitchInput {
    pub(crate) team_id: TeamId,
}

/// `POST /switch-team` — set the `active_team_id` cookie and redirect
/// back to the practices dashboard.
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) async fn switch_team_handler(
    jar: CookieJar,
    Form(input): Form<TeamSwitchInput>,
) -> (CookieJar, Redirect) {
    let jar = jar.add(
        axum_extra::extract::cookie::Cookie::build(("active_team_id", input.team_id.to_string()))
            .path("/")
            .http_only(true),
    );
    (jar, Redirect::to("/practices"))
}
