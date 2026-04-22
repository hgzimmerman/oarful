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
use std::collections::HashMap;

use lineup_db::app_user::{AppUser, NewAppUser, Role, UserId, UserRoleRow, UserStatus};
use lineup_db::schema::{app_user, user_invite, user_role};
use serde::Deserialize;

use crate::{
    handlers::ErrorResponse,
    state::{MailerCtx, TenantContext, TenantDb},
    templates,
};

// =====================================================================
// User list (PD only)
// =====================================================================

/// Build the users list markup (shared by `/users` and `/admin/users`).
pub(crate) async fn users_content(tenant: &TenantContext) -> Result<maud::Markup, ErrorResponse> {
    let (users, roles, user_rower_map) = tenant
        .db
        .with_conn(|conn| {
            let users: Vec<AppUser> = app_user::table
                .select(AppUser::as_select())
                .order(app_user::name.asc())
                .get_results(conn)?;
            let role_rows: Vec<UserRoleRow> = user_role::table
                .select(UserRoleRow::as_select())
                .get_results(conn)?;
            let roles: HashMap<UserId, Role> =
                role_rows.into_iter().map(|r| (r.user_id, r.role)).collect();
            let user_rower: HashMap<UserId, lineup_db::rower::types::RowerId> = users
                .iter()
                .filter_map(|u| u.rower_id.map(|rid| (u.id, rid)))
                .collect();
            Ok((users, roles, user_rower))
        })
        .await
        .map_err(super::internal_error)?;
    Ok(templates::users::list_content(
        &users,
        &roles,
        &user_rower_map,
    ))
}

// =====================================================================
// Invite creation (PD only)
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct InviteInput {
    pub(crate) email: String,
    pub(crate) name: String,
    pub(crate) role: Role,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn invite_handler(
    State(mailer): State<MailerCtx>,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<InviteInput>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let email = input.email.trim().to_lowercase();
    let name = input.name.trim().to_string();
    if email.is_empty() || name.is_empty() {
        let content = templates::users::invite_result(None, Some("Email and name are required."));
        return Ok(super::maybe_page_authed("Invite", content, hx, &tenant));
    }

    let role = input.role;
    let token = generate_token();
    let token_for_db = token.clone();
    let email_for_mailer = email.clone();
    let name_for_mailer = name.clone();

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
                    status: UserStatus::Invited,
                    created_at: now,
                    updated_at: now,
                },
            )?;
            AppUser::set_role(conn, user.id, role)?;

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
        Ok(new_user_id) => {
            let invite_path = format!("/invite/{}/{token}", tenant.config.tenant_slug);
            let invite_url = mailer.full_url(&invite_path);

            // Best-effort delivery — failure is logged but doesn't
            // block the invite (the UI still shows the link).
            // Trial tenants: skip email silently.
            if tenant.config.can_send_email() {
                if let Err(err) = mailer
                    .mailer
                    .send_invite(&email_for_mailer, &name_for_mailer, &invite_url)
                    .await
                {
                    tracing::warn!(?err, %email_for_mailer, "mailer failed to send invite");
                }
            }

            crate::audit::record(
                &tenant.db,
                tenant.claims.audit_user_id(),
                "invite.create",
                "user",
                &new_user_id.to_string(),
                Some(serde_json::json!({"role": input.role}).to_string()),
            );

            let content = templates::users::invite_result(Some(&invite_url), None);
            Ok(super::maybe_page_authed(
                "Invite sent",
                content,
                hx,
                &tenant,
            ))
        }
        Err(msg) => {
            let content = templates::users::invite_result(None, Some(&msg));
            Ok(super::maybe_page_authed("Invite", content, hx, &tenant))
        }
    }
}

// =====================================================================
// Resend invite (PD only)
// =====================================================================

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn resend_invite_handler(
    State(mailer): State<MailerCtx>,
    Extension(tenant): Extension<TenantContext>,
    Path(user_id): Path<UserId>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let token = generate_token();
    let token_for_db = token.clone();

    let user = tenant
        .db
        .with_conn(move |conn| {
            let user =
                AppUser::get(conn, user_id)?.ok_or_else(|| diesel::result::Error::NotFound)?;
            if user.status != UserStatus::Invited {
                return Err(diesel::result::Error::NotFound);
            }

            // Delete any existing invite token for this user.
            diesel::delete(user_invite::table.filter(user_invite::user_id.eq(user_id)))
                .execute(conn)?;

            // Insert fresh token.
            let expires = chrono::Utc::now().naive_utc() + chrono::TimeDelta::try_days(7).unwrap();
            diesel::insert_into(user_invite::table)
                .values((
                    user_invite::token_hash.eq(&token_for_db),
                    user_invite::user_id.eq(user_id),
                    user_invite::expires_at.eq(expires),
                ))
                .execute(conn)?;

            Ok(user)
        })
        .await
        .map_err(super::internal_error)?;

    let invite_path = format!("/invite/{}/{token}", tenant.config.tenant_slug);
    let invite_url = mailer.full_url(&invite_path);
    if tenant.config.can_send_email() {
        if let Err(err) = mailer
            .mailer
            .send_invite(&user.email, &user.name, &invite_url)
            .await
        {
            tracing::warn!(?err, email = %user.email, "mailer failed to resend invite");
        }
    }

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "invite.resend",
        "user",
        &user_id.to_string(),
        None,
    );

    // Return the updated row for HTMX swap. We need the roles map
    // and rower link for rendering.
    let (roles, rower_map) = tenant
        .db
        .with_conn(move |conn| {
            let mut role_map = HashMap::new();
            if let Some(role) = AppUser::role(conn, user_id)? {
                role_map.insert(user_id, role);
            }
            let mut rower_map = HashMap::new();
            if let Some(u) = AppUser::get(conn, user_id)? {
                if let Some(rid) = u.rower_id {
                    rower_map.insert(user_id, rid);
                }
            }
            Ok((role_map, rower_map))
        })
        .await
        .map_err(super::internal_error)?;

    Ok(Html(
        templates::users::user_row(&user, &roles, &rower_map).into_string(),
    ))
}

// =====================================================================
// Invite acceptance (public — no auth required)
// =====================================================================

/// `GET /invite/{token}` — password-set form.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn accept_page(
    State(tdb): State<TenantDb>,
    Path(token): Path<String>,
) -> Result<Html<String>, ErrorResponse> {
    // Scan all tenants for the invite token (the invite URL doesn't
    // encode which tenant it belongs to).
    if find_invite_tenant(&tdb, &token).await?.is_none() {
        return Ok(Html(
            templates::auth::login_page(Some("Invite link is invalid or expired.")).into_string(),
        ));
    }
    Ok(Html(
        templates::users::accept_form(&format!("/invite/{token}"), None).into_string(),
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct AcceptInput {
    pub(crate) password: String,
    pub(crate) password_confirm: String,
}

/// `POST /invite/{token}` — set password + activate.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn accept_handler(
    State(tdb): State<TenantDb>,
    Path(token): Path<String>,
    Form(input): Form<AcceptInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    if input.password.len() < 8 {
        return Ok(Html(
            templates::users::accept_form(
                &format!("/invite/{token}"),
                Some("Password must be at least 8 characters."),
            )
            .into_string(),
        )
        .into_response());
    }
    if input.password != input.password_confirm {
        return Ok(Html(
            templates::users::accept_form(
                &format!("/invite/{token}"),
                Some("Passwords do not match."),
            )
            .into_string(),
        )
        .into_response());
    }

    // Hash password on blocking pool.
    let password = input.password.clone();
    let hash = tokio::task::spawn_blocking(move || bcrypt::hash(password, bcrypt::DEFAULT_COST))
        .await
        .map_err(super::internal_error)?
        .map_err(super::internal_error)?;

    // Find which tenant owns this invite token.
    let Some(db) = find_invite_tenant(&tdb, &token).await? else {
        return Ok(Html(
            templates::auth::login_page(Some("Invite link is invalid or expired.")).into_string(),
        )
        .into_response());
    };

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

            diesel::delete(user_invite::table.filter(user_invite::token_hash.eq(&token_for_db)))
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
// Slug-prefixed invite routes
// =====================================================================

/// `GET /invite/{slug}/{token}` — password-set form (resolves tenant by slug).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn accept_page_with_slug(
    State(tdb): State<TenantDb>,
    Path((slug, token)): Path<(String, String)>,
) -> Result<Html<String>, ErrorResponse> {
    let Ok((_tid, db, _config)) = tdb.tenant_db_by_slug(&slug).await else {
        return Ok(Html(
            templates::auth::login_page(Some("Invite link is invalid or expired.")).into_string(),
        ));
    };
    if !validate_invite(&db, &token).await? {
        return Ok(Html(
            templates::auth::login_page(Some("Invite link is invalid or expired.")).into_string(),
        ));
    }
    Ok(Html(
        templates::users::accept_form(&format!("/invite/{slug}/{token}"), None).into_string(),
    ))
}

/// `POST /invite/{slug}/{token}` — set password + activate (resolves tenant by slug).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn accept_handler_with_slug(
    State(tdb): State<TenantDb>,
    Path((slug, token)): Path<(String, String)>,
    Form(input): Form<AcceptInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let action = format!("/invite/{slug}/{token}");
    if input.password.len() < 8 {
        return Ok(Html(
            templates::users::accept_form(&action, Some("Password must be at least 8 characters."))
                .into_string(),
        )
        .into_response());
    }
    if input.password != input.password_confirm {
        return Ok(Html(
            templates::users::accept_form(&action, Some("Passwords do not match.")).into_string(),
        )
        .into_response());
    }

    let password = input.password.clone();
    let hash = tokio::task::spawn_blocking(move || bcrypt::hash(password, bcrypt::DEFAULT_COST))
        .await
        .map_err(super::internal_error)?
        .map_err(super::internal_error)?;

    let Ok((_tid, db, _config)) = tdb.tenant_db_by_slug(&slug).await else {
        return Ok(Html(
            templates::auth::login_page(Some("Invite link is invalid or expired.")).into_string(),
        )
        .into_response());
    };

    let token_for_db = token.clone();
    let result = db
        .with_conn(move |conn| {
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

            diesel::delete(user_invite::table.filter(user_invite::token_hash.eq(&token_for_db)))
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

/// Scan all tenants to find which one owns an invite token.
async fn find_invite_tenant(
    tdb: &TenantDb,
    token: &str,
) -> Result<Option<lineup_db::state::Db>, ErrorResponse> {
    use lineup_master_db::tenant::Tenant;

    let tenants = tdb
        .master_db
        .with_conn(|conn| Tenant::list_all(conn))
        .await
        .map_err(super::internal_error)?;

    for t in &tenants {
        let (db, _config) = tdb.tenant_db(t.id).await.map_err(super::internal_error)?;
        if validate_invite(&db, token).await? {
            return Ok(Some(db));
        }
    }
    Ok(None)
}

async fn validate_invite(db: &lineup_db::state::Db, token: &str) -> Result<bool, ErrorResponse> {
    let token = token.to_string();
    db.with_conn(move |conn| {
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
pub(crate) fn require_at_least_role(
    claims: &crate::jwt::Claims,
    min: Role,
) -> Result<(), ErrorResponse> {
    let role = claims.role();
    if role.at_least(min) {
        Ok(())
    } else {
        Err(ErrorResponse(
            StatusCode::FORBIDDEN,
            "You don't have permission to perform this action.".into(),
        ))
    }
}

/// `POST /users/{id}/toggle-status` — toggle active ↔ disabled.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn toggle_status_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(user_id): Path<UserId>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let (user, roles, user_rower_map) = tenant
        .db
        .with_conn(move |conn| {
            let user = AppUser::get(conn, user_id)?.ok_or(diesel::result::Error::NotFound)?;
            let new_status = match user.status {
                lineup_db::app_user::UserStatus::Active => {
                    lineup_db::app_user::UserStatus::Disabled
                }
                lineup_db::app_user::UserStatus::Disabled => {
                    lineup_db::app_user::UserStatus::Active
                }
                _ => return Ok((user, HashMap::new(), HashMap::new())),
            };
            AppUser::set_status(conn, user_id, new_status)?;
            let user = AppUser::get(conn, user_id)?.ok_or(diesel::result::Error::NotFound)?;
            let role_rows: Vec<UserRoleRow> = user_role::table
                .select(UserRoleRow::as_select())
                .get_results(conn)?;
            let roles: HashMap<UserId, Role> =
                role_rows.into_iter().map(|r| (r.user_id, r.role)).collect();
            let user_rower: HashMap<UserId, lineup_db::rower::types::RowerId> = vec![user.clone()]
                .iter()
                .filter_map(|u| u.rower_id.map(|rid| (u.id, rid)))
                .collect();
            Ok((user, roles, user_rower))
        })
        .await
        .map_err(super::internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "user.toggle_status",
        "user",
        &user_id.to_string(),
        Some(serde_json::json!({"new_status": user.status}).to_string()),
    );

    Ok(Html(
        templates::users::user_row(&user, &roles, &user_rower_map).into_string(),
    ))
}

/// Generate a random 32-hex-char invite token.
pub(crate) fn generate_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let a = RandomState::new().build_hasher().finish();
    let b = RandomState::new().build_hasher().finish();
    format!("{a:016x}{b:016x}")
}
