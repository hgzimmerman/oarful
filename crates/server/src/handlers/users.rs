//! User management + invite flow.
//!
//! - `GET /users` — list all users in the tenant (PD only)
//! - `POST /users/invite` — create a user + invite token, show the link
//! - `GET /invite/{token}` — password-set form (public)
//! - `POST /invite/{token}` — set password, activate account (public)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    Extension, Form,
};
use axum_htmx::HxRequest;
use chrono::Utc;
use diesel::prelude::*;
use lineup_db::app_user::{AppUser, NewAppUser, Role, UserId};
use lineup_db::rower::Rower;
use lineup_db::schema::{app_user, user_invite};
use serde::Deserialize;

use crate::{state::{AppState, TenantContext}, templates};

// =====================================================================
// User list (PD only)
// =====================================================================

pub(crate) async fn list_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let users = tenant
        .db
        .with_conn(|conn| {
            app_user::table
                .select(AppUser::as_select())
                .order(app_user::name.asc())
                .get_results(conn)
        })
        .await
        .map_err(super::internal_error)?;
    let content = templates::users::list_content(&users);
    Ok(super::maybe_page("Users", content, hx))
}

// =====================================================================
// Invite creation (PD only)
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct InviteInput {
    pub(crate) email: String,
    pub(crate) name: String,
    pub(crate) role: String,
}

pub(crate) async fn invite_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<InviteInput>,
) -> Result<Html<String>, StatusCode> {
    require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let email = input.email.trim().to_lowercase();
    let name = input.name.trim().to_string();
    if email.is_empty() || name.is_empty() {
        let content = templates::users::invite_result(None, Some("Email and name are required."));
        return Ok(super::maybe_page("Invite", content, hx));
    }

    let role = Role::from_str(&input.role).unwrap_or(Role::Member);
    let token = generate_token();
    let token_for_db = token.clone();

    let result = tenant
        .db
        .with_conn(move |conn| {
            // Check for existing user with this email.
            if AppUser::find_by_email(conn, &email)?.is_some() {
                return Ok(Err("A user with this email already exists.".to_string()));
            }

            let now = Utc::now().naive_utc();
            let user = AppUser::create(
                conn,
                NewAppUser {
                    email,
                    password_hash: None,
                    name,
                    status: "invited".to_string(),
                    created_at: now,
                    updated_at: now,
                },
            )?;
            AppUser::set_role(conn, user.id, role)?;

            // Auto-link rower → user if a rower with the same email
            // exists. This lets the rower access self-service pages
            // after accepting the invite.
            if let Some(rower) = Rower::find_by_email(conn, &user.email)? {
                Rower::link_to_user(conn, rower.id, user.id.as_int())?;
            }

            // Store invite token.
            let expires = now + chrono::TimeDelta::try_days(7).unwrap();
            diesel::insert_into(user_invite::table)
                .values((
                    user_invite::token_hash.eq(&token_for_db),
                    user_invite::user_id.eq(user.id),
                    user_invite::expires_at.eq(expires),
                ))
                .execute(conn)?;

            Ok(Ok(user.id))
        })
        .await
        .map_err(super::internal_error)?;

    match result {
        Ok(_user_id) => {
            let invite_url = format!("/invite/{token}");
            let content = templates::users::invite_result(Some(&invite_url), None);
            Ok(super::maybe_page("Invite sent", content, hx))
        }
        Err(msg) => {
            let content = templates::users::invite_result(None, Some(&msg));
            Ok(super::maybe_page("Invite", content, hx))
        }
    }
}

// =====================================================================
// Invite acceptance (public — no auth required)
// =====================================================================

/// `GET /invite/{token}` — password-set form.
pub(crate) async fn accept_page(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let db = state.tenant_db(state.default_tenant_id).await.map_err(super::internal_error)?;
    let valid = validate_invite(&db, &token).await?;
    if !valid {
        return Ok(Html(
            templates::auth::login_page(Some("Invite link is invalid or expired.")).into_string(),
        ));
    }
    Ok(Html(
        templates::users::accept_form(&token, None).into_string(),
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct AcceptInput {
    pub(crate) password: String,
    pub(crate) password_confirm: String,
}

/// `POST /invite/{token}` — set password + activate.
pub(crate) async fn accept_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(input): Form<AcceptInput>,
) -> Result<impl IntoResponse, StatusCode> {
    if input.password.len() < 8 {
        return Ok(Html(
            templates::users::accept_form(&token, Some("Password must be at least 8 characters."))
                .into_string(),
        )
        .into_response());
    }
    if input.password != input.password_confirm {
        return Ok(Html(
            templates::users::accept_form(&token, Some("Passwords do not match.")).into_string(),
        )
        .into_response());
    }

    // Hash password on blocking pool.
    let password = input.password.clone();
    let hash = tokio::task::spawn_blocking(move || {
        bcrypt::hash(password, bcrypt::DEFAULT_COST)
    })
    .await
    .map_err(super::internal_error)?
    .map_err(super::internal_error)?;

    let db = state.tenant_db(state.default_tenant_id).await.map_err(super::internal_error)?;
    let token_for_db = token.clone();
    let result = db
        .with_conn(move |conn| {
            // Look up and consume the invite.
            let invite: Option<(UserId, chrono::NaiveDateTime)> = user_invite::table
                .filter(user_invite::token_hash.eq(&token_for_db))
                .select((user_invite::user_id, user_invite::expires_at))
                .first(conn)
                .optional()?;

            let Some((user_id, expires)) = invite else {
                return Ok(false);
            };
            if expires < Utc::now().naive_utc() {
                return Ok(false);
            }

            AppUser::set_password_and_activate(conn, user_id, &hash)?;

            diesel::delete(
                user_invite::table.filter(user_invite::token_hash.eq(&token_for_db)),
            )
            .execute(conn)?;

            Ok(true)
        })
        .await
        .map_err(super::internal_error)?;

    if !result {
        return Ok(Html(
            templates::auth::login_page(Some("Invite link is invalid or expired.")).into_string(),
        )
        .into_response());
    }

    Ok(Redirect::to("/login").into_response())
}

// =====================================================================
// Helpers
// =====================================================================

async fn validate_invite(db: &lineup_db::state::Db, token: &str) -> Result<bool, StatusCode> {
    let token = token.to_string();
    db
        .with_conn(move |conn| {
            let row: Option<chrono::NaiveDateTime> = user_invite::table
                .filter(user_invite::token_hash.eq(&token))
                .select(user_invite::expires_at)
                .first(conn)
                .optional()?;

            Ok(row.map(|exp| exp > Utc::now().naive_utc()).unwrap_or(false))
        })
        .await
        .map_err(super::internal_error)
}

/// Check that the authenticated user has at least `min` role.
/// Returns 403 if insufficient. The check is ordinal: Member < Coach
/// < ProgramDirector, so `require_at_least_role(claims, Coach)` passes
/// for both Coach and ProgramDirector.
pub(crate) fn require_at_least_role(claims: &crate::jwt::Claims, min: Role) -> Result<(), StatusCode> {
    let role = claims.role().unwrap_or(Role::Member);
    if role.at_least(min) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Generate a random 32-hex-char invite token.
fn generate_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let a = RandomState::new().build_hasher().finish();
    let b = RandomState::new().build_hasher().finish();
    format!("{a:016x}{b:016x}")
}

