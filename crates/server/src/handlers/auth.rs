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
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use lineup_db::app_user::{AppUser, Role, UserStatus};
use lineup_db::magic_link::MagicLink;
use lineup_db::team::Team;
use lineup_master_db::tenant::TenantId;
use serde::Deserialize;

use crate::{
    magic_link::{create_magic_link, hash_token},
    state::AppState,
    templates,
};

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

#[derive(Debug, Deserialize)]
pub(crate) struct LoginPageQuery {
    #[serde(default)]
    reset: Option<String>,
}

/// `GET /login` — render the email form (step 1).
pub(crate) async fn login_page(
    jar: CookieJar,
    axum::extract::Query(query): axum::extract::Query<LoginPageQuery>,
) -> Html<String> {
    let prefill = jar.get(KNOWN_USER_COOKIE).map(|c| c.value().to_string());
    let has_demo = jar.get(super::demo::DEMO_SLUG_COOKIE).is_some();
    let success = if query.reset.is_some() {
        Some("Password updated. Sign in with your new password.")
    } else {
        None
    };
    Html(
        templates::auth::login_email_step(None, prefill.as_deref(), has_demo, success)
            .into_string(),
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
///
/// If the email matches `SUPERUSER_EMAIL`, skip the password step
/// and send a magic link directly.
pub(crate) async fn email_step_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(input): Form<EmailStepInput>,
) -> axum::response::Response {
    let email = input.email.trim().to_lowercase();
    if email.is_empty() {
        return Html(
            templates::auth::login_email_step(Some("Email is required."), None, false, None)
                .into_string(),
        )
        .into_response();
    }

    // Superuser: magic link only, no password step.
    if state.is_superuser_email(&email) {
        let token = state
            .jwt_keys
            .issue_superuser_magic_token()
            .unwrap_or_default();
        let url = state.full_url(&format!("/auth/su/{token}"));
        let clubs = vec![("Oarful Admin".to_string(), url)];
        if let Err(e) = state.mailer.send_magic_login(&email, "Admin", &clubs).await {
            tracing::warn!(?e, "failed to send superuser magic link");
        }
        return Html(templates::auth::login_magic_sent(&email).into_string()).into_response();
    }

    let show_magic = jar.get(KNOWN_USER_COOKIE).is_some();
    Html(templates::auth::login_password_step(&email, None, show_magic).into_string())
        .into_response()
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
) -> Result<impl IntoResponse, super::ErrorResponse> {
    let email = input.email.trim().to_lowercase();
    let password = input.password.clone();
    let show_magic = jar.get(KNOWN_USER_COOKIE).is_some();

    // Scan all tenant DBs for this email.
    let tenants = state
        .master_db
        .with_conn(lineup_master_db::tenant::Tenant::list_all)
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
                let default_team = Team::default_for_user(conn, user.id)?;
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
        return Ok(Html(
            templates::auth::login_password_step(&email, Some("Invalid credentials."), show_magic)
                .into_string(),
        )
        .into_response());
    }

    // Verify password against the first match. The password is
    // per-tenant but typically identical for the same email.
    let first = &matches[0];
    if first.user.status != UserStatus::Active {
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

    let hash: String = match &first.user.password_hash {
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

    // If multiple tenants, show the club picker. The password is
    // re-submitted as a hidden field so we can complete login after
    // the user picks a club.
    if matches.len() > 1 {
        let clubs: Vec<(i32, String, Option<String>)> = matches
            .iter()
            .map(|m| {
                (
                    m.tenant_id.as_int(),
                    m.tenant_name.clone(),
                    m.role.map(|r| r.as_str().to_string()),
                )
            })
            .collect();
        return Ok(Html(
            templates::auth::login_club_picker(&email, &input.password, &clubs).into_string(),
        )
        .into_response());
    }

    let m = matches.into_iter().next().unwrap();
    let role = m.role.unwrap_or(Role::Member);
    let team_id = m
        .default_team
        .map(|t| t.id)
        .unwrap_or(lineup_db::team::TeamId::new(1));
    let login_tenant_id = m.tenant_id;

    let token = state
        .jwt_keys
        .issue(m.user.id, login_tenant_id, role, team_id)
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
// Club picker (step 3 — multi-tenant)
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct PickInput {
    pub(crate) email: String,
    pub(crate) password: String,
    pub(crate) tenant_id: i32,
}

/// `POST /login/pick` — complete login after club selection.
///
/// Re-verifies credentials against the chosen tenant (the password was
/// already checked on the previous step, but we re-verify because it's
/// submitted as a hidden field and could be tampered with).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn pick_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(input): Form<PickInput>,
) -> Result<impl IntoResponse, super::ErrorResponse> {
    let email = input.email.trim().to_lowercase();
    let password = input.password.clone();
    let tenant_id = TenantId::new(input.tenant_id);

    let (db, _config) = state
        .tenant_db(tenant_id)
        .await
        .map_err(super::internal_error)?;

    let email_clone = email.clone();
    let found = db
        .with_conn(move |conn| {
            let Some(user) = AppUser::find_by_email(conn, &email_clone)? else {
                return Ok(None);
            };
            let role = AppUser::role(conn, user.id)?;
            let default_team = Team::default_for_user(conn, user.id)?;
            Ok(Some((user, role, default_team)))
        })
        .await
        .map_err(super::internal_error)?;

    let Some((user, role, default_team)) = found else {
        return Ok(
            Html(templates::auth::login_page(Some("Invalid credentials.")).into_string())
                .into_response(),
        );
    };

    let hash = match &user.password_hash {
        Some(h) => h.clone(),
        None => {
            return Ok(
                Html(templates::auth::login_page(Some("Password not set.")).into_string())
                    .into_response(),
            );
        }
    };

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
        .issue(user.id, tenant_id, role, team_id)
        .map_err(super::internal_error)?;

    let jwt_cookie = axum_extra::extract::cookie::Cookie::build((TOKEN_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax);

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
) -> Result<Html<String>, super::ErrorResponse> {
    let email = input.email.trim().to_lowercase();

    // Find all matching tenants. If none found, we still show the
    // confirmation page (don't leak account existence).
    let tenants = state
        .master_db
        .with_conn(lineup_master_db::tenant::Tenant::list_all)
        .await
        .map_err(super::internal_error)?;

    struct MatchedTenant {
        user: AppUser,
        tenant_id: TenantId,
        tenant_slug: String,
        tenant_name: String,
    }

    let mut matches = Vec::new();
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
            if u.status == UserStatus::Active {
                matches.push(MatchedTenant {
                    user: u,
                    tenant_id: t.id,
                    tenant_slug: t.slug.clone(),
                    tenant_name: t.name.clone(),
                });
            }
        }
    }

    if !matches.is_empty() {
        let expires_at =
            (chrono::Utc::now() + chrono::TimeDelta::try_hours(24).unwrap()).naive_utc();

        // Create one magic link per tenant and collect (club_name, url) pairs.
        let mut clubs: Vec<(String, String)> = Vec::new();
        let user_name = matches[0].user.name.clone();

        for m in &matches {
            let created = create_magic_link(m.user.id, "/practices", expires_at, None);
            let raw_token = created.raw_token.clone();
            let row = created.row;

            let (db, _config) = state
                .tenant_db(m.tenant_id)
                .await
                .map_err(super::internal_error)?;
            db.with_conn(move |conn| MagicLink::create(conn, row))
                .await
                .map_err(super::internal_error)?;

            let url = state.full_url(&format!("/auth/magic/{}/{raw_token}", m.tenant_slug));
            clubs.push((m.tenant_name.clone(), url));
        }

        if let Err(err) = state
            .mailer
            .send_magic_login(&email, &user_name, &clubs)
            .await
        {
            tracing::warn!(?err, %email, "failed to send magic login email");
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

/// `GET /auth/magic/{slug}/{token}` — validate a magic link, create a
/// JWT session, and redirect to the link's `redirect_path`.
///
/// The slug identifies the tenant whose DB holds the magic link.
/// If the user already has a valid JWT cookie, we keep it (don't
/// downgrade their session). If they don't, we issue a JWT that
/// expires at the magic link's `expires_at`.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn magic_link_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((slug, token)): Path<(String, String)>,
) -> Result<impl IntoResponse, super::ErrorResponse> {
    let token_hash = hash_token(&token);

    let (tenant_id, db, _config) = state
        .tenant_db_by_slug(&slug)
        .await
        .map_err(super::internal_error)?;

    let token_hash_clone = token_hash.clone();
    let link = db
        .with_conn(move |conn| MagicLink::validate(conn, &token_hash_clone))
        .await
        .map_err(super::internal_error)?;

    let Some(link) = link else {
        return Ok(Html(
            templates::auth::login_page(Some("Magic link is invalid or expired.")).into_string(),
        )
        .into_response());
    };

    let redirect_path = link.redirect_path.clone();
    let magic_user_id = link.user_id;
    let magic_expires_at = link.expires_at;
    let magic_team_id = link.team_id;

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
            let user = AppUser::get(conn, magic_user_id)?.ok_or(diesel::result::Error::NotFound)?;
            let role = AppUser::role(conn, magic_user_id)?;
            let default_team = Team::default_for_user(conn, user.id)?;
            Ok((user, role, default_team))
        })
        .await
        .map_err(super::internal_error)?;

    if user.status != UserStatus::Active {
        return Ok(Html(
            templates::auth::login_page(Some("Account is not active. Check your invite email."))
                .into_string(),
        )
        .into_response());
    }

    let role = role.unwrap_or(Role::Member);
    // Use the magic link's team_id if set, otherwise fall back to default.
    let team_id = magic_team_id
        .or_else(|| default_team.map(|t| t.id))
        .unwrap_or(lineup_db::team::TeamId::new(1));

    let jwt = state
        .jwt_keys
        .issue_with_expiry(user.id, tenant_id, role, team_id, exp_unix)
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
// Superuser magic link landing
// =====================================================================

/// `GET /auth/su/{token}` — verify the short-lived superuser JWT
/// embedded in the magic link, issue a full session JWT, redirect
/// to the admin panel.
#[tracing::instrument(level = "info", skip_all)]
pub(crate) async fn superuser_magic_link_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let _claims = match state.jwt_keys.verify(&token) {
        Ok(c) if c.is_superuser() => c,
        _ => return (jar, Redirect::to("/login")).into_response(),
    };

    // Issue a full 24h superuser session token.
    let jwt = match state.jwt_keys.issue_superuser() {
        Ok(t) => t,
        Err(_) => return (jar, Redirect::to("/login")).into_response(),
    };

    let jwt_cookie = axum_extra::extract::cookie::Cookie::build((TOKEN_COOKIE, jwt))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax);
    let jar = jar.add(jwt_cookie);
    (jar, Redirect::to("/su")).into_response()
}

// =====================================================================
// Logout
// =====================================================================

/// `POST /logout` — clear the JWT cookie (but keep known_user).
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) async fn logout_handler(jar: CookieJar) -> impl IntoResponse {
    let jar = jar.remove(axum_extra::extract::cookie::Cookie::build(TOKEN_COOKIE).path("/"));
    (jar, Redirect::to("/login"))
}

// =====================================================================
// Forgot / Reset password
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct ForgotPasswordQuery {
    #[serde(default)]
    email: Option<String>,
}

/// `GET /forgot-password` — render the "enter your email" form.
pub(crate) async fn forgot_password_page(
    axum::extract::Query(query): axum::extract::Query<ForgotPasswordQuery>,
) -> Html<String> {
    Html(templates::auth::forgot_password_page(query.email.as_deref()).into_string())
}

#[derive(Debug, Deserialize)]
pub(crate) struct ForgotPasswordInput {
    email: String,
}

/// `POST /forgot-password` — send a password-reset magic link.
///
/// Always shows the "sent" confirmation regardless of whether the email
/// exists, to avoid leaking account existence.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn forgot_password_handler(
    State(state): State<AppState>,
    Form(input): Form<ForgotPasswordInput>,
) -> Result<Html<String>, super::ErrorResponse> {
    let email = input.email.trim().to_lowercase();

    let tenants = state
        .master_db
        .with_conn(lineup_master_db::tenant::Tenant::list_all)
        .await
        .map_err(super::internal_error)?;

    struct MatchedTenant {
        user: AppUser,
        tenant_id: TenantId,
        tenant_slug: String,
        tenant_name: String,
    }

    let mut matches = Vec::new();
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
            if u.status == UserStatus::Active {
                matches.push(MatchedTenant {
                    user: u,
                    tenant_id: t.id,
                    tenant_slug: t.slug.clone(),
                    tenant_name: t.name.clone(),
                });
            }
        }
    }

    if !matches.is_empty() {
        // 1-hour expiry for password reset links.
        let expires_at =
            (chrono::Utc::now() + chrono::TimeDelta::try_hours(1).unwrap()).naive_utc();

        let mut clubs: Vec<(String, String)> = Vec::new();
        let user_name = matches[0].user.name.clone();

        for m in &matches {
            let created = crate::magic_link::create_magic_link(
                m.user.id,
                "/reset-password",
                expires_at,
                None,
            );
            let raw_token = created.raw_token.clone();
            let row = created.row;

            let (db, _config) = state
                .tenant_db(m.tenant_id)
                .await
                .map_err(super::internal_error)?;
            db.with_conn(move |conn| MagicLink::create(conn, row))
                .await
                .map_err(super::internal_error)?;

            let url = state.full_url(&format!("/auth/magic/{}/{raw_token}", m.tenant_slug));
            clubs.push((m.tenant_name.clone(), url));
        }

        if let Err(err) = state
            .mailer
            .send_password_reset(&email, &user_name, &clubs)
            .await
        {
            tracing::warn!(?err, %email, "failed to send password reset email");
        }
    } else {
        tracing::debug!(%email, "password reset requested for unknown/inactive email");
    }

    Ok(Html(
        templates::auth::forgot_password_sent(&email).into_string(),
    ))
}

/// `GET /reset-password` — render the new-password form. Protected route.
pub(crate) async fn reset_password_page() -> Html<String> {
    Html(templates::auth::reset_password_form(None).into_string())
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResetPasswordInput {
    password: String,
    password_confirm: String,
}

/// `POST /reset-password` — set a new password. Protected route.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn reset_password_handler(
    Extension(tenant): Extension<crate::state::TenantContext>,
    jar: CookieJar,
    Form(input): Form<ResetPasswordInput>,
) -> Result<impl IntoResponse, super::ErrorResponse> {
    if input.password.len() < 8 {
        return Ok(Html(
            templates::auth::reset_password_form(Some("Password must be at least 8 characters."))
                .into_string(),
        )
        .into_response());
    }
    if input.password != input.password_confirm {
        return Ok(Html(
            templates::auth::reset_password_form(Some("Passwords do not match.")).into_string(),
        )
        .into_response());
    }

    let password = input.password.clone();
    let hash = tokio::task::spawn_blocking(move || bcrypt::hash(password, bcrypt::DEFAULT_COST))
        .await
        .map_err(super::internal_error)?
        .map_err(super::internal_error)?;

    let user_id = tenant
        .claims
        .user_id()
        .ok_or_else(|| super::bad_request("Not available in superuser view."))?;
    tenant
        .db
        .with_conn(move |conn| AppUser::set_password(conn, user_id, &hash))
        .await
        .map_err(super::internal_error)?;

    // Clear JWT so user must re-login with new password.
    let jar = jar.remove(axum_extra::extract::cookie::Cookie::build(TOKEN_COOKIE).path("/"));
    Ok((jar, Redirect::to("/login?reset=1")).into_response())
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

    // Check billing status — expired trials and suspended tenants
    // see a "renew" page instead of the app. Allow /logout and /my
    // so users can still manage their account.
    if !config.is_billing_ok() {
        let path = req.uri().path().to_string();
        if path != "/logout" && !path.starts_with("/my") && path != "/reset-password" {
            return Html(
                crate::templates::billing::suspended_page(
                    &config.tenant_name,
                    &state.webmaster_email,
                )
                .into_string(),
            )
            .into_response();
        }
    }

    let ctx = crate::state::TenantContext {
        db,
        tenant_id,
        claims,
        config,
    };
    req.extensions_mut().insert(ctx);
    next.run(req).await
}
