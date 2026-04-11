//! Authentication handlers: login page, login POST, logout, and the
//! `require_auth` middleware that gates all protected routes.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use axum_extra::extract::CookieJar;
use lineup_db::app_user::{AppUser, Role, UserStatus};
use lineup_db::team::Team;
use lineup_master_db::tenant::TenantId;
use serde::Deserialize;

use crate::{state::AppState, templates};

/// Cookie name for the JWT token.
pub(crate) const TOKEN_COOKIE: &str = "token";

// =====================================================================
// Login
// =====================================================================

/// `GET /login` — render the login form.
pub(crate) async fn login_page(jar: CookieJar) -> Html<String> {
    // If already logged in, the middleware won't even reach here for
    // protected routes, but someone can navigate to /login directly.
    let _ = jar; // might use later for flash messages
    Html(templates::auth::login_page(None).into_string())
}

#[derive(Debug, Deserialize)]
pub(crate) struct LoginInput {
    pub(crate) email: String,
    pub(crate) password: String,
}

/// `POST /login` — verify credentials, issue JWT cookie, redirect.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn login_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(input): Form<LoginInput>,
) -> Result<impl IntoResponse, StatusCode> {
    let email = input.email.trim().to_lowercase();
    let password = input.password.clone();

    // Scan all tenant DBs for this email. If found in multiple,
    // the user picks which club to log into.
    let tenants = state
        .master_db
        .with_conn(|conn| lineup_master_db::tenant::Tenant::list_all(conn))
        .await
        .map_err(super::internal_error)?;

    struct Match {
        tenant_id: TenantId,
        tenant_name: String,
        user: AppUser,
        role: Option<Role>,
        default_team: Option<Team>,
    }

    let mut matches = Vec::new();
    for t in &tenants {
        let Ok((db, _)) = state.tenant_db(t.id).await else {
            continue;
        };
        let email_clone = email.clone();
        let found: Option<(AppUser, Option<Role>, Option<Team>)> = db
            .with_conn(move |conn| {
                let Some(user) = AppUser::find_by_email(conn, &email_clone)? else {
                    return Ok(None);
                };
                let role = AppUser::role(conn, user.id)?;
                let default_team = Team::first(conn)?;
                Ok(Some((user, role, default_team)))
            })
            .await
            .map_err(super::internal_error)?;
        if let Some((user, role, default_team)) = found {
            matches.push(Match {
                tenant_id: t.id,
                tenant_name: t.name.clone(),
                user,
                role,
                default_team,
            });
        }
    }

    if matches.is_empty() {
        return Ok(
            Html(templates::auth::login_page(Some("Invalid credentials.")).into_string())
                .into_response(),
        );
    }

    // For now, use the first match (single-tenant deployments).
    // TODO: if matches.len() > 1, render a club picker.
    let m = matches.into_iter().next().unwrap();
    let user = m.user;
    let role = m.role;
    let default_team = m.default_team;
    let login_tenant_id = m.tenant_id;

    if user.parsed_status() != Some(UserStatus::Active) {
        return Ok(
            Html(
                templates::auth::login_page(Some(
                    "Account is not active. Check your invite email.",
                ))
                .into_string(),
            )
            .into_response(),
        );
    }

    let hash: String = match &user.password_hash {
        Some(h) => h.clone(),
        None => {
            return Ok(
                Html(
                    templates::auth::login_page(Some(
                        "Password not set. Check your invite email.",
                    ))
                    .into_string(),
                )
                .into_response(),
            );
        }
    };

    // Verify password (bcrypt is CPU-bound; run on blocking pool).
    let ok = tokio::task::spawn_blocking(move || bcrypt::verify(password, &hash))
        .await
        .map_err(super::internal_error)?
        .map_err(super::internal_error)?;

    if !ok {
        return Ok(
            Html(templates::auth::login_page(Some("Invalid credentials.")).into_string())
                .into_response(),
        );
    }

    let role = role.unwrap_or(Role::Member);
    let team_id = default_team
        .map(|t| t.id)
        .unwrap_or(lineup_db::team::TeamId::new(1));

    let token = state
        .jwt_keys
        .issue(user.id, login_tenant_id, role, team_id)
        .map_err(super::internal_error)?;

    let cookie = axum_extra::extract::cookie::Cookie::build((TOKEN_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax);

    let jar = jar.add(cookie);
    Ok((jar, Redirect::to("/practices")).into_response())
}

// =====================================================================
// Logout
// =====================================================================

/// `POST /logout` — clear the JWT cookie.
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) async fn logout_handler(jar: CookieJar) -> impl IntoResponse {
    let jar = jar.remove(
        axum_extra::extract::cookie::Cookie::build(TOKEN_COOKIE).path("/"),
    );
    (jar, Redirect::to("/login"))
}

// =====================================================================
// Auth middleware
// =====================================================================

/// Axum middleware that validates the JWT cookie on every request.
/// If missing or invalid, redirects to `/login`. On success, resolves
/// the tenant DB from the cache and injects a `TenantContext` into
/// request extensions so handlers can extract it.
pub(crate) async fn require_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Response {
    let token = jar
        .get(TOKEN_COOKIE)
        .map(|c| c.value().to_string());

    let claims = match token {
        Some(t) => match state.jwt_keys.verify(&t) {
            Ok(c) => c,
            Err(_) => return Redirect::to("/login").into_response(),
        },
        None => return Redirect::to("/login").into_response(),
    };

    // Resolve the tenant DB for this request.
    let tenant_id = claims.tenant_id();
    let (db, attributes_public) = match state.tenant_db(tenant_id).await {
        Ok(pair) => pair,
        Err(err) => {
            tracing::error!(?err, %tenant_id, "failed to resolve tenant DB");
            return Redirect::to("/login").into_response();
        }
    };

    let ctx = crate::state::TenantContext {
        db,
        tenant_id,
        claims,
        attributes_public,
    };
    req.extensions_mut().insert(ctx);
    next.run(req).await
}
