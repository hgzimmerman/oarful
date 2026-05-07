//! HTTP handlers. Each submodule owns its own routes and templates;
//! [`create_router`] wires them all together.

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{delete, get, post},
    Extension, Form, Router,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use lineup_db::team::TeamId;
use maud::Markup;
use serde::Deserialize;

use axum::extract::Query;

use crate::{
    state::{AppState, TenantContext},
    templates,
};

pub(crate) mod admin;
pub(crate) mod audit;
pub(crate) mod auth;
pub(crate) mod billing;
pub(crate) mod boats;
pub(crate) mod demo;
pub(crate) mod history;
pub(crate) mod my;
pub(crate) mod practices;
pub(crate) mod rowers;
pub(crate) mod signup;
pub(crate) mod solve;
pub(crate) mod stripe_webhook;
pub(crate) mod superuser;
pub(crate) mod sync;
pub(crate) mod team_hub;
pub(crate) mod teams;
pub(crate) mod timeline;
pub(crate) mod unsubscribe;
pub(crate) mod users;

/// Compose the full route table. Called from [`crate::build_router`] so
/// the binary doesn't need to know about individual handlers.
pub(crate) fn create_router(state: AppState) -> Router {
    // Public routes — no auth required.
    let public = Router::new()
        .route("/", get(landing_handler))
        .route("/login", get(auth::login_page).post(auth::login_handler))
        .route("/login/email", post(auth::email_step_handler))
        .route("/login/magic", post(auth::magic_login_handler))
        .route("/login/pick", post(auth::pick_handler))
        .route("/logout", post(auth::logout_handler))
        .route(
            "/signup",
            get(signup::signup_page).post(signup::signup_handler),
        )
        .route(
            "/invite/{token}",
            get(users::accept_page).post(users::accept_handler),
        )
        .route(
            "/invite/{slug}/{token}",
            get(users::accept_page_with_slug).post(users::accept_handler_with_slug),
        )
        .route("/auth/magic/{slug}/{token}", get(auth::magic_link_handler))
        .route("/auth/su/{token}", get(auth::superuser_magic_link_handler))
        .route("/demo", post(demo::create_demo_handler))
        .route(
            "/demo/resume",
            get(demo::resume_demo_handler).post(demo::resume_demo_handler),
        )
        .route(
            "/forgot-password",
            get(auth::forgot_password_page).post(auth::forgot_password_handler),
        )
        .route(
            "/unsubscribe/{slug}/{user_id}/{email_type}/{signature}",
            get(unsubscribe::unsubscribe_handler).post(unsubscribe::unsubscribe_post_handler),
        );

    // Stripe webhook — public (no auth, Stripe signature is the auth).
    let public = if state.stripe_ctx.is_some() {
        public.route("/stripe/webhook", post(stripe_webhook::webhook_handler))
    } else {
        public
    };

    let public = public.with_state(state.clone());

    // Superuser routes — require superuser JWT (no tenant context).
    let su_routes = Router::new()
        .route("/su", get(superuser::index_handler))
        .route("/su/billing/{id}", post(superuser::billing_handler))
        .route("/su/create-tenant", post(superuser::create_tenant_handler))
        .route("/su/impersonate/{id}", post(superuser::impersonate_handler))
        .route("/su/exit", post(superuser::exit_handler))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            superuser::require_superuser,
        ))
        .with_state(state.clone());

    // Protected routes — require a valid JWT cookie.
    let protected = Router::new()
        .route(
            "/practices",
            get(practices::list_handler).post(practices::create_handler),
        )
        .route("/practices/planning", get(practices::planning_redirect))
        .route("/practices/committed", get(practices::committed_redirect))
        .route(
            "/practices/reminder-preview",
            get(practices::reminder_preview_handler),
        )
        .route(
            "/practices/send-reminders",
            post(practices::send_reminders_handler),
        )
        .route(
            "/practices/lineup-preview",
            get(practices::lineup_preview_handler),
        )
        .route(
            "/practices/send-lineups",
            post(practices::send_lineups_handler),
        )
        .route("/practices/{id}/cancel", post(practices::cancel_handler))
        .route(
            "/practices/{id}/dismiss-plan",
            post(practices::dismiss_plan_handler),
        )
        .route("/solve/{id}", get(solve::view_handler))
        .route("/solve/{id}/stream", get(solve::stream_handler))
        .route("/solve/{id}/editor", get(solve::editor_handler))
        .route("/solve/{id}/preset-bar", get(solve::preset_bar_handler))
        .route("/solver-profile", post(solve::save_profile_handler))
        .route("/solver-profile/edit", get(solve::edit_profile_handler))
        .route(
            "/solver-profile/{name}",
            delete(solve::delete_profile_handler),
        )
        .route("/commit/{id}", post(solve::commit_handler))
        .route("/commit-lineup/{id}", post(solve::commit_lineup_handler))
        .route("/draft-lineup/{id}", post(solve::draft_lineup_handler))
        .route("/clear-draft/{id}", post(solve::clear_draft_handler))
        .route("/history", get(history::list_handler))
        .route("/history/{id}", get(history::detail_handler))
        .route("/history/{id}/notes", post(history::notes_handler))
        .route("/history/{id}/timeline/edit", get(timeline::open_editor))
        .route("/history/{id}/timeline/add", post(timeline::add_block))
        .route(
            "/history/{id}/timeline/delete",
            post(timeline::delete_block),
        )
        .route(
            "/history/{id}/timeline/patch-block",
            post(timeline::patch_block),
        )
        .route(
            "/history/{id}/timeline/patch-segment",
            post(timeline::patch_segment),
        )
        .route(
            "/history/{id}/timeline/target",
            post(timeline::update_target),
        )
        .route("/history/{id}/timeline/save", post(timeline::save_timeline))
        .route("/history/{id}/timeline/close", post(timeline::close_editor))
        .route(
            "/history/{id}/timeline/reorder",
            post(timeline::reorder_block),
        )
        .route(
            "/history/{id}/timeline/duplicate",
            post(timeline::duplicate_block),
        )
        .route(
            "/history/{id}/timeline/group-add",
            post(timeline::group_add_segment),
        )
        .route(
            "/history/{id}/timeline/group-delete",
            post(timeline::group_delete_segment),
        )
        .route(
            "/history/{id}/timeline/group-patch",
            post(timeline::group_patch),
        )
        .route(
            "/history/{id}/timeline/group-reorder",
            post(timeline::group_reorder_segment),
        )
        .route(
            "/history/{id}/timeline/template",
            post(timeline::insert_template),
        )
        // Club hub (Coach+)
        .route("/team", get(team_hub::index_handler))
        .route("/team/roster", get(team_hub::roster_handler))
        .route("/team/attendance", get(team_hub::attendance_handler))
        .route(
            "/team/roster/batch-invite",
            post(rowers::batch_invite_handler),
        )
        .route(
            "/team/attendance/toggle",
            post(team_hub::attendance_toggle_handler),
        )
        .route(
            "/team/sync",
            get(team_hub::sync_handler).post(sync::sync_handler),
        )
        // Admin hub (PD+)
        .route("/admin", get(admin::index_handler))
        .route("/admin/users", get(admin::users_handler))
        .route("/admin/teams", get(admin::teams_handler))
        .route(
            "/admin/roster",
            get(admin::roster_handler).post(teams::roster_matrix_save_handler),
        )
        .route("/admin/fleet", get(admin::fleet_handler))
        .route("/admin/fleet/boats", get(admin::fleet_boats_handler))
        .route(
            "/admin/fleet/defaults",
            get(admin::fleet_defaults_handler).post(teams::fleet_matrix_save_handler),
        )
        .route("/admin/audit", get(admin::audit_handler))
        .route(
            "/admin/settings",
            get(admin::settings_handler).post(admin::settings_update_handler),
        )
        .route("/admin/export", get(admin::export_handler))
        .route(
            "/admin/restore",
            get(admin::restore_form_handler).post(admin::restore_handler),
        )
        .route(
            "/admin/restore/confirm",
            post(admin::restore_confirm_handler),
        )
        .route("/audit/rows", get(audit::rows_handler))
        // Old list URLs → redirect to new hubs (keep POSTs working)
        .route("/club", get(|| async { Redirect::permanent("/team") }))
        .route(
            "/rowers",
            get(|| async { Redirect::permanent("/team/roster") }),
        )
        .route(
            "/boats",
            get(|| async { Redirect::permanent("/admin/fleet") }).post(boats::create_handler),
        )
        .route(
            "/team/fleet",
            get(|| async { Redirect::permanent("/admin/fleet") }),
        )
        .route(
            "/sync",
            get(|| async { Redirect::permanent("/team/sync") }).post(sync::sync_handler),
        )
        .route(
            "/users",
            get(|| async { Redirect::permanent("/admin/users") }),
        )
        .route(
            "/audit",
            get(|| async { Redirect::permanent("/admin/audit") }),
        )
        .route(
            "/teams",
            get(|| async { Redirect::permanent("/admin/teams") }).post(teams::create_handler),
        )
        // Detail routes (unchanged)
        .route("/boats/new", get(boats::new_handler))
        .route("/boats/export.csv", get(boats::export_csv_handler))
        .route(
            "/boats/usage-matrix.csv",
            get(boats::usage_matrix_csv_handler),
        )
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
        .route(
            "/rowers/{id}/edit-attributes",
            get(rowers::edit_attributes_handler),
        )
        .route(
            "/rowers/{id}/toggle-active",
            post(rowers::toggle_active_handler),
        )
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
        .route("/rowers/{id}/erg-test", post(rowers::erg_test_add_handler))
        .route(
            "/rowers/{id}/erg-test/{test_id}",
            delete(rowers::erg_test_delete_handler),
        )
        .route("/users/invite", post(users::invite_handler))
        .route(
            "/users/{id}/resend-invite",
            post(users::resend_invite_handler),
        )
        .route(
            "/users/{id}/toggle-status",
            post(users::toggle_status_handler),
        )
        .route(
            "/teams/{id}",
            get(teams::detail_handler).post(teams::update_handler),
        )
        .route(
            "/teams/{id}/toggle-archive",
            post(teams::toggle_archive_handler),
        )
        .route(
            "/teams/{id}/thresholds",
            post(teams::threshold_save_handler),
        )
        .route("/teams/{id}/histogram", get(teams::histogram_handler))
        .route("/teams/selector", get(teams::selector_handler))
        .route("/onboarding/dismiss", post(onboarding_dismiss_handler))
        .route("/nav/stale-badge", get(stale_badge_handler))
        // My pages
        .route("/my", get(my::index_handler))
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
        .route("/my/erg-test", post(my::erg_test_add_handler))
        .route(
            "/reset-password",
            get(auth::reset_password_page).post(auth::reset_password_handler),
        )
        .route("/switch-team", post(switch_team_handler))
        .route("/confirm", get(confirm_handler));

    // Stripe billing routes — only mounted when STRIPE_SECRET_KEY is set.
    let protected = if state.stripe_ctx.is_some() {
        protected
            .route("/billing/checkout", post(billing::checkout_handler))
            .route("/billing/success", get(billing::success_handler))
            .route("/billing/status", get(billing::status_handler))
            .route("/billing/portal", get(billing::portal_handler))
    } else {
        protected
    };

    let protected = protected
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .with_state(state);

    public.merge(su_routes).merge(protected)
}

/// `GET /` — landing page for unauthenticated users, redirect for
/// authenticated ones.
async fn landing_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> axum::response::Response {
    // If the user has a valid JWT, send them straight to the app.
    if let Some(token) = jar.get(auth::TOKEN_COOKIE) {
        if state.jwt_keys.verify(token.value()).is_ok() {
            return Redirect::to("/practices").into_response();
        }
    }
    let stripe_enabled = state.stripe_ctx.is_some();
    Html(templates::landing::landing_page(state.signup_disabled, stripe_enabled).into_string())
        .into_response()
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
        Html(
            templates::layout::page(
                title,
                content,
                tenant.claims.role(),
                tenant.claims.is_superuser(),
            )
            .into_string(),
        )
    }
}

/// Error response: status code + plain-text message body.
/// The layout's `htmx:beforeSwap` listener reads the body for the toast.
/// Implements `Display` (for `#[tracing::instrument(err)]`) and
/// `IntoResponse` (for axum handler returns).
pub(crate) struct ErrorResponse(pub StatusCode, pub String);

impl std::fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.0, self.1)
    }
}

impl std::fmt::Debug for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ErrorResponse({}, {:?})", self.0, self.1)
    }
}

impl axum::response::IntoResponse for ErrorResponse {
    fn into_response(self) -> axum::response::Response {
        (self.0, self.1).into_response()
    }
}

/// Collapse an anyhow/diesel/etc. error into a 500 response and log it.
/// Handlers use `.map_err(internal_error)` as their escape hatch.
pub(crate) fn internal_error<E: std::fmt::Debug>(error: E) -> ErrorResponse {
    tracing::error!(?error, "handler error");
    ErrorResponse(
        StatusCode::INTERNAL_SERVER_ERROR,
        "An unexpected error occurred.".into(),
    )
}

pub(crate) fn bad_request(msg: impl Into<String>) -> ErrorResponse {
    ErrorResponse(StatusCode::BAD_REQUEST, msg.into())
}

pub(crate) fn not_found(msg: impl Into<String>) -> ErrorResponse {
    ErrorResponse(StatusCode::NOT_FOUND, msg.into())
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
) -> Result<TeamId, ErrorResponse> {
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
        .with_conn(lineup_db::team::Team::first)
        .await
        .map_err(internal_error)?;
    team.map(|t| t.id).ok_or_else(|| {
        tracing::error!("no teams in the database");
        ErrorResponse(StatusCode::INTERNAL_SERVER_ERROR, "No teams found.".into())
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

/// `POST /onboarding/dismiss` — mark the onboarding checklist as dismissed
/// for the current user.
#[tracing::instrument(level = "debug", skip_all, err)]
async fn onboarding_dismiss_handler(
    Extension(tenant): Extension<TenantContext>,
) -> Result<Html<String>, ErrorResponse> {
    users::require_at_least_role(&tenant.claims, lineup_db::app_user::Role::Coach)?;
    let user_id = tenant
        .claims
        .user_id()
        .ok_or_else(|| internal_error(anyhow::anyhow!("no user id")))?;
    tenant
        .db
        .with_conn(move |conn| {
            lineup_db::onboarding::complete_step(
                conn,
                user_id,
                lineup_db::onboarding::OnboardingStep::Dismissed,
            )
        })
        .await
        .map_err(internal_error)?;
    Ok(Html(String::new()))
}

/// `GET /nav/stale-badge` — returns a small count badge if any upcoming
/// committed lineups have availability changes, or empty HTML otherwise.
/// Called via `hx-trigger="load"` from the navbar Practices link.
#[tracing::instrument(level = "debug", skip_all, err)]
async fn stale_badge_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
) -> Result<Html<String>, ErrorResponse> {
    use lineup_db::{
        app_user::Role, availability::Availability, lineup::Lineup, practice::Practice, team::Team,
    };

    let role = tenant.claims.role();
    if !role.at_least(Role::Coach) {
        return Ok(Html(String::new()));
    }

    let team_id = active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let today = chrono::Utc::now().date_naive();

    let stale_count: usize = tenant
        .db
        .with_conn(move |conn| {
            let assume_available = Team::get(conn, team_id)?
                .map(|t| t.assume_available.as_bool())
                .unwrap_or(false);

            // Upcoming committed practices only.
            let practices: Vec<Practice> = Practice::list_committed(conn, team_id)?
                .into_iter()
                .filter(|p| p.date >= today)
                .collect();
            if practices.is_empty() {
                return Ok(0usize);
            }

            let pids: Vec<_> = practices.iter().map(|p| p.id).collect();
            let committed_rowers = Lineup::committed_rower_ids_for_practices(conn, &pids)?;
            let avail = Availability::map_for_practices(conn, &pids)?;

            let count = committed_rowers
                .iter()
                .filter(|(pid, rower_ids)| {
                    rower_ids.iter().any(|rid| {
                        !avail
                            .get(&(*rid, **pid))
                            .map(|s| s.is_available())
                            .unwrap_or(assume_available)
                    })
                })
                .count();
            Ok(count)
        })
        .await
        .map_err(internal_error)?;

    if stale_count == 0 {
        return Ok(Html(String::new()));
    }

    Ok(Html(
        maud::html! {
            span class="ml-1.5 inline-flex items-center justify-center w-5 h-5 text-xs font-bold leading-none text-white bg-amber-500 rounded-full" {
                (stale_count)
            }
        }
        .into_string(),
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ConfirmQuery {
    kind: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

/// `GET /confirm?kind=...&id=...` — render a styled confirmation modal.
pub(crate) async fn confirm_handler(
    Query(q): Query<ConfirmQuery>,
) -> Result<Html<String>, ErrorResponse> {
    use maud::html;
    let id = q.id.as_deref().unwrap_or("");
    let name = q.name.as_deref().unwrap_or("");

    let delete_msg = format!("Delete preset \"{name}\"? This cannot be undone.");
    let (title, message, action) = match q.kind.as_str() {
        "archive-team" => (
            "Archive team",
            "Archive this team? It will be hidden from the team switcher for non-PD users.",
            html! {
                form method="post" action={"/teams/" (id) "/toggle-archive"}
                     hx-post={"/teams/" (id) "/toggle-archive"}
                     hx-target="#content"
                     onclick="releaseFocus(); document.getElementById('confirm-modal').remove(); document.getElementById('confirm-modal-backdrop').remove()" {
                    button type="submit"
                           class="px-4 py-2 text-sm font-semibold text-white bg-red-600 hover:bg-red-700 rounded shadow transition" {
                        "Archive"
                    }
                }
            },
        ),
        "deactivate-rower" => (
            "Deactivate rower",
            "Deactivate this rower? They will be hidden from the roster and ineligible for lineups.",
            html! {
                form method="post" action={"/rowers/" (id) "/toggle-active"}
                     hx-post={"/rowers/" (id) "/toggle-active"}
                     hx-target="#content"
                     onclick="releaseFocus(); document.getElementById('confirm-modal').remove(); document.getElementById('confirm-modal-backdrop').remove()" {
                    button type="submit"
                           class="px-4 py-2 text-sm font-semibold text-white bg-red-600 hover:bg-red-700 rounded shadow transition" {
                        "Deactivate"
                    }
                }
            },
        ),
        "delete-preset" => (
            "Delete preset",
            delete_msg.as_str(),
            html! {
                button type="button"
                       hx-delete={"/solver-profile/" (name)}
                       hx-target="#content"
                       onclick="releaseFocus(); document.getElementById('confirm-modal').remove(); document.getElementById('confirm-modal-backdrop').remove()"
                       class="px-4 py-2 text-sm font-semibold text-white bg-red-600 hover:bg-red-700 rounded shadow transition" {
                    "Delete"
                }
            },
        ),
        "restore-backup" => (
            "Restore from backup",
            "This will overwrite ALL current data with the backup. Are you sure?",
            html! {
                button type="submit" form="restore-form"
                       onclick="releaseFocus(); document.getElementById('confirm-modal').remove(); document.getElementById('confirm-modal-backdrop').remove()"
                       class="px-4 py-2 text-sm font-semibold text-white bg-red-600 hover:bg-red-700 rounded shadow transition" {
                    "Restore"
                }
            },
        ),
        _ => return Err(bad_request("Unknown confirmation type.")),
    };

    Ok(Html(
        templates::confirm_modal::confirm_modal(title, message, action).into_string(),
    ))
}
