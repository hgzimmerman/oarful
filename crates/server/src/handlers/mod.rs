//! HTTP handlers. Each submodule owns its own routes and templates;
//! [`create_router`] wires them all together.

use axum::{
    http::StatusCode,
    response::{Html, Redirect},
    routing::{delete, get, post},
    Form, Router,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use lineup_db::team::TeamId;
use maud::Markup;
use serde::Deserialize;

use crate::{state::AppState, templates};

pub(crate) mod admin;
pub(crate) mod audit;
pub(crate) mod auth;
pub(crate) mod boats;
pub(crate) mod team_hub;
pub(crate) mod demo;
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
        .route("/login/email", post(auth::email_step_handler))
        .route("/login/magic", post(auth::magic_login_handler))
        .route("/login/pick", post(auth::pick_handler))
        .route("/logout", post(auth::logout_handler))
        .route(
            "/invite/{token}",
            get(users::accept_page).post(users::accept_handler),
        )
        .route("/auth/magic/{slug}/{token}", get(auth::magic_link_handler))
        .route("/demo", post(demo::create_demo_handler))
        .route("/demo/resume", get(demo::resume_demo_handler).post(demo::resume_demo_handler))
        .with_state(state.clone());

    // Protected routes — require a valid JWT cookie.
    let protected = Router::new()
        .route("/", get(|| async { Redirect::permanent("/practices") }))
        .route("/practices", get(practices::list_handler).post(practices::create_handler))
        .route("/practices/planning", get(practices::planning_handler))
        .route("/practices/committed", get(practices::committed_handler))
        .route("/practices/reminder-preview", get(practices::reminder_preview_handler))
        .route("/practices/send-reminders", post(practices::send_reminders_handler))
        .route("/practices/send-lineups", post(practices::send_lineups_handler))
        .route("/practices/{id}/cancel", post(practices::cancel_handler))
        .route("/solve/{id}", get(solve::view_handler))
        .route("/solve/{id}/editor", get(solve::editor_handler))
        .route("/solve/{id}/preset-bar", get(solve::preset_bar_handler))
        .route("/solver-profile", post(solve::save_profile_handler))
        .route("/solver-profile/edit", get(solve::edit_profile_handler))
        .route("/solver-profile/{name}", delete(solve::delete_profile_handler))
        .route("/commit/{id}", post(solve::commit_handler))
        .route("/commit-lineup/{id}", post(solve::commit_lineup_handler))
        .route("/history", get(history::list_handler))
        .route("/history/{id}", get(history::detail_handler))
        .route("/history/{id}/notes", post(history::notes_handler))
        // Club hub (Coach+)
        .route("/team", get(team_hub::index_handler))
        .route("/team/roster", get(team_hub::roster_handler))
        .route("/team/attendance", get(team_hub::attendance_handler))
        .route("/team/roster/batch-invite", post(rowers::batch_invite_handler))
        .route("/team/sync", get(team_hub::sync_handler).post(sync::sync_handler))
        // Admin hub (PD+)
        .route("/admin", get(admin::index_handler))
        .route("/admin/users", get(admin::users_handler))
        .route("/admin/teams", get(admin::teams_handler))
        .route("/admin/roster", get(admin::roster_handler).post(teams::roster_matrix_save_handler))
        .route("/admin/fleet", get(admin::fleet_handler))
        .route("/admin/fleet/boats", get(admin::fleet_boats_handler))
        .route("/admin/fleet/defaults", get(admin::fleet_defaults_handler).post(teams::fleet_matrix_save_handler))
        .route("/admin/audit", get(admin::audit_handler))
        .route("/admin/export", get(admin::export_handler))
        .route("/admin/restore", get(admin::restore_form_handler).post(admin::restore_handler))
        .route("/admin/restore/confirm", post(admin::restore_confirm_handler))
        .route("/audit/rows", get(audit::rows_handler))
        // Old list URLs → redirect to new hubs (keep POSTs working)
        .route("/club", get(|| async { Redirect::permanent("/team") }))
        .route("/rowers", get(|| async { Redirect::permanent("/team/roster") }))
        .route("/boats", get(|| async { Redirect::permanent("/admin/fleet") }).post(boats::create_handler))
        .route("/team/fleet", get(|| async { Redirect::permanent("/admin/fleet") }))
        .route("/sync", get(|| async { Redirect::permanent("/team/sync") }).post(sync::sync_handler))
        .route("/users", get(|| async { Redirect::permanent("/admin/users") }))
        .route("/audit", get(|| async { Redirect::permanent("/admin/audit") }))
        .route("/teams", get(|| async { Redirect::permanent("/admin/teams") }).post(teams::create_handler))
        // Detail routes (unchanged)
        .route("/boats/new", get(boats::new_handler))
        .route("/boats/export.csv", get(boats::export_csv_handler))
        .route(
            "/boats/{id}",
            get(boats::detail_handler)
                .put(boats::update_handler)
                .post(boats::update_handler),
        )
        .route("/boats/{id}/edit", get(boats::edit_handler))
        .route(
            "/rowers/{id}",
            get(rowers::detail_handler).post(rowers::update_handler),
        )
        .route("/rowers/{id}/attributes", get(rowers::attributes_handler))
        .route("/rowers/{id}/edit-attributes", get(rowers::edit_attributes_handler))
        .route("/rowers/{id}/toggle-active", post(rowers::toggle_active_handler))
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
        .route("/users/invite", post(users::invite_handler))
        .route(
            "/users/{id}/resend-invite",
            post(users::resend_invite_handler),
        )
        .route("/teams/{id}", get(teams::detail_handler).post(teams::update_handler))
        .route("/teams/{id}/toggle-archive", post(teams::toggle_archive_handler))
        .route("/teams/selector", get(teams::selector_handler))
        // My pages
        .route(
            "/my/profile",
            get(my::profile_handler).post(my::profile_update_handler),
        )
        .route(
            "/my/availability",
            get(my::availability_handler).post(my::availability_update_handler),
        )
        .route(
            "/my/email-preferences",
            get(my::email_prefs_handler).post(my::email_prefs_update_handler),
        )
        .route("/switch-team", post(switch_team_handler))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .with_state(state);

    public.merge(protected)
}


pub(crate) fn maybe_page_authed(
    title: &str,
    content: Markup,
    HxRequest(is_htmx): HxRequest,
    tenant: &crate::state::TenantContext,
) -> Html<String> {
    if is_htmx {
        Html(content.into_string())
    } else {
        Html(templates::layout::page(title, content, tenant.claims.role()).into_string())
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
    // Cookie takes priority — set by POST /switch-team.
    if let Some(cookie) = jar.get("active_team_id") {
        if let Ok(id) = cookie.value().parse::<TeamId>() {
            return Ok(id);
        }
    }
    // Fall back to the JWT's default team (set at login).
    if let Some(c) = claims {
        return Ok(c.team_id());
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
