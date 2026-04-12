//! Authentication handlers: two-step login flow, magic-link login,
//! logout, and the `require_auth` middleware.
//!
//! Login flow:
//! 1. `GET /login` — email form (step 1)
//! 2. `POST /login/email` — render password page (step 2)
//! 3. `POST /login` — verify password, issue JWT
//! 4. `POST /login/magic` — send magic-link email (returning users only)

use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use axum_extra::extract::CookieJar;
use lineup_db::app_user::{AppUser, Role, UserStatus};
use lineup_db::magic_link::MagicLink;
use lineup_db::team::Team;
use lineup_master_db::tenant::TenantId;
use serde::Deserialize;

use crate::{magic_link::{create_magic_link, hash_token}, state::AppState, templates};

/// Cookie name for the JWT token.
pub(crate) const TOKEN_COOKIE: &str = "token";

/// Long-lived cookie set on successful login. Its presence enables
/// the "email me a sign-in link" button on the password step.
/// Value is the user's email (for prefilling the email field).
const KNOWN_USER_COOKIE: &str = "known_user";

/// Max-age for the known_user cookie: ~1 year.
const KNOWN_USER_MAX_AGE: time::Duration = time::Duration::days(365);

// =====================================================================
// Step 1: Email form
// =====================================================================

/// `GET /login` — render the email form (step 1).
pub(crate) async fn login_page(jar: CookieJar) -> Html<String> {
    let prefill = jar.get(KNOWN_USER_COOKIE).map(|c| c.value().to_string());
    Html(
        templates::auth::login_email_step(None, prefill.as_deref()).into_string(),
    )
}

// =====================================================================
// Step 2: Password form
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct EmailStepInput {
    pub(crate) email: String,
}

/// `POST /login/email` — validate the email was provided, render the
/// password page. Always renders step 2 regardless of whether the
/// email exists (to avoid leaking account existence).
pub(crate) async fn email_step_handler(
    jar: CookieJar,
    Form(input): Form<EmailStepInput>,
) -> Html<String> {
    let email = input.email.trim().to_lowercase();
    if email.is_empty() {
        return Html(
            templates::auth::login_email_step(Some("Email is required."), None).into_string(),
        );
    }
    let show_magic = jar.get(KNOWN_USER_COOKIE).is_some();
    Html(
        templates::auth::login_password_step(&email, None, show_magic).into_string(),
    )
}

// =====================================================================
// Step 3: Password verification
// =====================================================================

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
    let show_magic = jar.get(KNOWN_USER_COOKIE).is_some();

    // Scan all tenant DBs for this email.
    let tenants = state
        .master_db
        .with_conn(|conn| lineup_master_db::tenant::Tenant::list_all(conn))
        .await
        .map_err(super::internal_error)?;

    struct Match {
        tenant_id: TenantId,
        _tenant_name: String,
        user: AppUser,
        role: Option<Role>,
        default_team: Option<Team>,
    }

    let mut matches = Vec::new();
    for t in &tenants {
        let Ok((db, _config)) = state.tenant_db(t.id).await else {
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
                _tenant_name: t.name.clone(),
                user,
                role,
                default_team,
            });
        }
    }

    if matches.is_empty() {
        return Ok(Html(
            templates::auth::login_password_step(&email, Some("Invalid credentials."), show_magic)
                .into_string(),
        )
        .into_response());
    }

    // For now, use the first match (single-tenant deployments).
    // TODO: if matches.len() > 1, render a club picker.
    let m = matches.into_iter().next().unwrap();
    let user = m.user;
    let role = m.role;
    let default_team = m.default_team;
    let login_tenant_id = m.tenant_id;

    if user.parsed_status() != Some(UserStatus::Active) {
        return Ok(Html(
            templates::auth::login_password_step(
                &email,
                Some("Account is not active. Check your invite email."),
                show_magic,
            )
            .into_string(),
        )
        .into_response());
    }

    let hash: String = match &user.password_hash {
        Some(h) => h.clone(),
        None => {
            return Ok(Html(
                templates::auth::login_password_step(
                    &email,
                    Some("Password not set. Check your invite email."),
                    show_magic,
                )
                .into_string(),
            )
            .into_response());
        }
    };

    // Verify password (bcrypt is CPU-bound; run on blocking pool).
    let ok = tokio::task::spawn_blocking(move || bcrypt::verify(password, &hash))
        .await
        .map_err(super::internal_error)?
        .map_err(super::internal_error)?;

    if !ok {
        return Ok(Html(
            templates::auth::login_password_step(&email, Some("Invalid credentials."), show_magic)
                .into_string(),
        )
        .into_response());
    }

    let role = role.unwrap_or(Role::Member);
    let team_id = default_team
        .map(|t| t.id)
        .unwrap_or(lineup_db::team::TeamId::new(1));

    let token = state
        .jwt_keys
        .issue(user.id, login_tenant_id, role, team_id)
        .map_err(super::internal_error)?;

    let jwt_cookie = axum_extra::extract::cookie::Cookie::build((TOKEN_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax);

    // Set the long-lived known_user cookie so this user gets the
    // magic-link option on future visits.
    let known_cookie = axum_extra::extract::cookie::Cookie::build((KNOWN_USER_COOKIE, email))
        .path("/")
        .http_only(true)
        .max_age(KNOWN_USER_MAX_AGE)
        .same_site(axum_extra::extract::cookie::SameSite::Lax);

    let jar = jar.add(jwt_cookie).add(known_cookie);
    Ok((jar, Redirect::to("/practices")).into_response())
}

// =====================================================================
// Magic-link login (send link to email)
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct MagicLoginInput {
    pub(crate) email: String,
}

/// `POST /login/magic` — send a magic-link sign-in email.
///
/// Always shows the "link sent" confirmation regardless of whether
/// the email exists, to avoid leaking account existence.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn magic_login_handler(
    State(state): State<AppState>,
    Form(input): Form<MagicLoginInput>,
) -> Result<Html<String>, StatusCode> {
    let email = input.email.trim().to_lowercase();

    // Try to find the user. If not found, still show the confirmation
    // page (don't leak account existence).
    let tenants = state
        .master_db
        .with_conn(|conn| lineup_master_db::tenant::Tenant::list_all(conn))
        .await
        .map_err(super::internal_error)?;

    let mut found_user: Option<(AppUser, TenantId)> = None;
    for t in &tenants {
        let Ok((db, _config)) = state.tenant_db(t.id).await else {
            continue;
        };
        let email_clone = email.clone();
        let user = db
            .with_conn(move |conn| AppUser::find_by_email(conn, &email_clone))
            .await
            .map_err(super::internal_error)?;
        if let Some(u) = user {
            if u.parsed_status() == Some(UserStatus::Active) {
                found_user = Some((u, t.id));
                break;
            }
        }
    }

    if let Some((user, tenant_id)) = found_user {
        // Create a magic link with 24h expiry, redirecting to home.
        let expires_at = (chrono::Utc::now() + chrono::TimeDelta::try_hours(24).unwrap())
            .naive_utc();
        let created = create_magic_link(user.id, "/practices", expires_at);
        let raw_token = created.raw_token.clone();
        let row = created.row;

        let (db, _config) = state
            .tenant_db(tenant_id)
            .await
            .map_err(super::internal_error)?;
        db.with_conn(move |conn| MagicLink::create(conn, row))
            .await
            .map_err(super::internal_error)?;

        let magic_url = state.full_url(&format!("/auth/magic/{raw_token}"));
        if let Err(err) = state
            .mailer
            .send_invite(&user.email, &user.name, &magic_url)
            .await
        {
            tracing::warn!(?err, email = %user.email, "failed to send magic login link");
        }
    } else {
        tracing::debug!(%email, "magic login requested for unknown/inactive email");
    }

    // Always show success to avoid leaking email existence.
    Ok(Html(
        templates::auth::login_magic_sent(&email).into_string(),
    ))
}

// =====================================================================
// Magic link token authentication
// =====================================================================

/// `GET /auth/magic/{token}` — validate a magic link, create a JWT
/// session, and redirect to the link's `redirect_path`.
///
/// If the user already has a valid JWT cookie, we keep it (don't
/// downgrade their session). If they don't, we issue a JWT that
/// expires at the magic link's `expires_at`.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn magic_link_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let token_hash = hash_token(&token);

    // For now, use the default tenant (single-tenant). Multi-tenant
    // would scan or encode tenant_id in the link.
    let (db, _config) = state
        .tenant_db(state.default_tenant_id)
        .await
        .map_err(super::internal_error)?;

    let token_hash_clone = token_hash.clone();
    let link = db
        .with_conn(move |conn| MagicLink::validate(conn, &token_hash_clone))
        .await
        .map_err(super::internal_error)?;

    let Some(link) = link else {
        return Ok(
            Html(
                templates::auth::login_page(Some("Magic link is invalid or expired."))
                    .into_string(),
            )
            .into_response(),
        );
    };

    let redirect_path = link.redirect_path.clone();
    let magic_user_id = link.user_id;
    let magic_expires_at = link.expires_at;

    // Consume the token so it can't be replayed.
    let token_hash_clone = token_hash.clone();
    db.with_conn(move |conn| MagicLink::consume(conn, &token_hash_clone))
        .await
        .map_err(super::internal_error)?;

    // Check if the user already has a valid JWT — if so, just redirect.
    let existing_valid = jar
        .get(TOKEN_COOKIE)
        .and_then(|c| state.jwt_keys.verify(c.value()).ok())
        .is_some();

    if existing_valid {
        return Ok(Redirect::to(&redirect_path).into_response());
    }

    // No valid JWT — issue one expiring when the magic link expires.
    let exp_unix = magic_expires_at.and_utc().timestamp() as u64;

    let (user, role, default_team) = db
        .with_conn(move |conn| {
            let user = AppUser::get(conn, magic_user_id)?
                .ok_or(diesel::result::Error::NotFound)?;
            let role = AppUser::role(conn, magic_user_id)?;
            let default_team = Team::first(conn)?;
            Ok((user, role, default_team))
        })
        .await
        .map_err(super::internal_error)?;

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

    let role = role.unwrap_or(Role::Member);
    let team_id = default_team
        .map(|t| t.id)
        .unwrap_or(lineup_db::team::TeamId::new(1));

    let jwt = state
        .jwt_keys
        .issue_with_expiry(user.id, state.default_tenant_id, role, team_id, exp_unix)
        .map_err(super::internal_error)?;

    let jwt_cookie = axum_extra::extract::cookie::Cookie::build((TOKEN_COOKIE, jwt))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax);

    // Also set the known_user cookie for future magic-link convenience.
    let known_cookie =
        axum_extra::extract::cookie::Cookie::build((KNOWN_USER_COOKIE, user.email.clone()))
            .path("/")
            .http_only(true)
            .max_age(KNOWN_USER_MAX_AGE)
            .same_site(axum_extra::extract::cookie::SameSite::Lax);

    let jar = jar.add(jwt_cookie).add(known_cookie);
    Ok((jar, Redirect::to(&redirect_path)).into_response())
}

// =====================================================================
// Logout
// =====================================================================

/// `POST /logout` — clear the JWT cookie (but keep known_user).
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
    let token = jar.get(TOKEN_COOKIE).map(|c| c.value().to_string());

    let claims = match token {
        Some(t) => match state.jwt_keys.verify(&t) {
            Ok(c) => c,
            Err(_) => return Redirect::to("/login").into_response(),
        },
        None => return Redirect::to("/login").into_response(),
    };

    // Resolve the tenant DB for this request.
    let tenant_id = claims.tenant_id();
    let (db, config) = match state.tenant_db(tenant_id).await {
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
        config,
    };
    req.extensions_mut().insert(ctx);
    next.run(req).await
}
