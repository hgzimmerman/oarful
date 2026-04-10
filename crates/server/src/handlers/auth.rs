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
pub(crate) async fn login_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(input): Form<LoginInput>,
) -> Result<impl IntoResponse, StatusCode> {
    let email = input.email.trim().to_lowercase();
    let password = input.password.clone();

    // Look up user + role in the tenant DB.
    let found = state
        .db
        .with_conn(move |conn| {
            let user = AppUser::find_by_email(conn, &email)?;
            let role = match &user {
                Some(u) => AppUser::role(conn, u.id)?,
                None => None,
            };
            let default_team = Team::first(conn)?;
            Ok((user, role, default_team))
        })
        .await
        .map_err(super::internal_error)?;

    let (user, role, default_team) = found;

    // Validate.
    let user = match user {
        Some(u) => u,
        None => {
            return Ok(
                Html(templates::auth::login_page(Some("Invalid credentials.")).into_string())
                    .into_response(),
            );
        }
    };

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

    let hash = match &user.password_hash {
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
        .issue(user.id, state.tenant_id, role, team_id)
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
/// If missing or invalid, redirects to `/login`. On success, injects
/// `Claims` into request extensions so handlers can extract it.
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

    // Stash claims in request extensions for downstream handlers.
    req.extensions_mut().insert(claims);
    next.run(req).await
}
