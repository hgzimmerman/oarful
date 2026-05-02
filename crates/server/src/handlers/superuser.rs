//! Superuser admin panel: tenant list, billing management, impersonation.

use axum::{
    extract::{Path, Request, State},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use lineup_db::app_user::{AppUser, NewAppUser, Role, UserStatus};
use lineup_db::team::{NewTeam, Team};
use lineup_master_db::tenant::{BillingStatus, NewTenant, Tenant, TenantId};
use serde::Deserialize;

use crate::state::AppState;
use crate::templates;

/// Cookie name for preserving the superuser session during impersonation.
const SU_TOKEN_COOKIE: &str = "su_token";

// =====================================================================
// Middleware
// =====================================================================

/// Middleware: requires a valid JWT with `is_superuser == true` and
/// no tenant context (tenant_id == 0). Injects `Claims` into
/// request extensions.
pub(crate) async fn require_superuser(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Response {
    let token = jar
        .get(super::auth::TOKEN_COOKIE)
        .map(|c| c.value().to_string());

    let claims = match token {
        Some(t) => match state.jwt_keys.verify(&t) {
            Ok(c) if c.is_superuser() && c.tenant_id() == TenantId::new(0) => c,
            _ => return Redirect::to("/login").into_response(),
        },
        None => return Redirect::to("/login").into_response(),
    };

    req.extensions_mut().insert(claims);
    next.run(req).await
}

// =====================================================================
// Handlers
// =====================================================================

/// `GET /su` — superuser dashboard with tenant list.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn index_handler(
    State(state): State<AppState>,
) -> Result<Html<String>, super::ErrorResponse> {
    let tenants = state
        .master_db
        .with_conn(Tenant::list_all)
        .await
        .map_err(super::internal_error)?;

    Ok(Html(
        templates::superuser::su_dashboard(&tenants).into_string(),
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct BillingInput {
    status: BillingStatus,
}

/// `POST /su/billing/{id}` — update a tenant's billing status.
#[tracing::instrument(level = "info", skip_all, err)]
pub(crate) async fn billing_handler(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Form(input): Form<BillingInput>,
) -> Result<Html<String>, super::ErrorResponse> {
    let tenant_id = TenantId::new(id);
    let status = input.status;

    state
        .master_db
        .with_conn(move |conn| Tenant::set_billing_status(conn, tenant_id, status))
        .await
        .map_err(super::internal_error)?;

    // Evict from cache so the billing check picks up the change.
    state.evict_tenant(tenant_id);

    // Re-fetch and return the updated row for HTMX swap.
    let tenant = state
        .master_db
        .with_conn(move |conn| Tenant::get(conn, tenant_id))
        .await
        .map_err(super::internal_error)?
        .ok_or_else(|| super::not_found("Tenant not found."))?;

    Ok(Html(
        templates::superuser::su_tenant_row(&tenant).into_string(),
    ))
}

/// `POST /su/impersonate/{id}` — view a tenant as PD (synthetic identity).
#[tracing::instrument(level = "info", skip_all, err)]
pub(crate) async fn impersonate_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<i32>,
) -> Result<impl IntoResponse, super::ErrorResponse> {
    let tenant_id = TenantId::new(id);

    // Save the current superuser token so we can restore it on exit.
    let su_jwt = jar
        .get(super::auth::TOKEN_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or_else(|| super::bad_request("No session."))?;

    // Open the tenant DB to find the first team.
    let (db, _config) = state
        .tenant_db(tenant_id)
        .await
        .map_err(super::internal_error)?;

    let team_id = db
        .with_conn(move |conn| {
            let team = Team::first(conn)?;
            Ok(team
                .map(|t| t.id)
                .unwrap_or(lineup_db::team::TeamId::new(1)))
        })
        .await
        .map_err(super::internal_error)?;

    // Issue a tenant-view JWT with synthetic user_id=0 and PD role.
    // No real user account is hijacked.
    let jwt = state
        .jwt_keys
        .issue_superuser_tenant_view(tenant_id, team_id)
        .map_err(super::internal_error)?;

    let jwt_cookie = axum_extra::extract::cookie::Cookie::build((super::auth::TOKEN_COOKIE, jwt))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax);

    let su_cookie = axum_extra::extract::cookie::Cookie::build((SU_TOKEN_COOKIE, su_jwt))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax);

    let jar = jar.add(jwt_cookie).add(su_cookie);
    Ok((jar, Redirect::to("/practices")))
}

/// `POST /su/exit` — exit impersonation, restore superuser session.
#[tracing::instrument(level = "info", skip_all, err)]
pub(crate) async fn exit_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<impl IntoResponse, super::ErrorResponse> {
    let su_jwt = jar
        .get(SU_TOKEN_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or_else(|| super::bad_request("No superuser session to restore."))?;

    // Verify it's actually a superuser token.
    let claims = state
        .jwt_keys
        .verify(&su_jwt)
        .map_err(|_| super::bad_request("Invalid superuser session."))?;
    if !claims.is_superuser() {
        return Err(super::bad_request("Invalid superuser session."));
    }

    // Restore the superuser JWT as the active token.
    let jwt_cookie =
        axum_extra::extract::cookie::Cookie::build((super::auth::TOKEN_COOKIE, su_jwt))
            .path("/")
            .http_only(true)
            .same_site(axum_extra::extract::cookie::SameSite::Lax);

    // Remove the su_token cookie.
    let jar = jar
        .add(jwt_cookie)
        .remove(axum_extra::extract::cookie::Cookie::build(SU_TOKEN_COOKIE).path("/"));

    Ok((jar, Redirect::to("/su")))
}

// =====================================================================
// Create grandfathered tenant
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct CreateTenantInput {
    club_name: String,
    admin_name: String,
    admin_email: String,
    admin_password: String,
}

/// `POST /su/create-tenant` — create a new tenant with grandfathered billing.
#[tracing::instrument(level = "info", skip_all, err)]
pub(crate) async fn create_tenant_handler(
    State(state): State<AppState>,
    Form(input): Form<CreateTenantInput>,
) -> Result<impl IntoResponse, super::ErrorResponse> {
    let club_name = input.club_name.trim().to_string();
    if club_name.is_empty() || club_name.len() > 100 {
        return Err(super::bad_request(
            "Club name is required (max 100 characters).",
        ));
    }
    let admin_name = input.admin_name.trim().to_string();
    if admin_name.is_empty() {
        return Err(super::bad_request("Admin name is required."));
    }
    let email = input.admin_email.trim().to_lowercase();
    if !email.contains('@') || !email.contains('.') {
        return Err(super::bad_request("Invalid email address."));
    }
    let password = input.admin_password;
    if password.len() < 8 {
        return Err(super::bad_request(
            "Password must be at least 8 characters.",
        ));
    }

    // Generate unique slug.
    let base_slug = super::signup::slugify(&club_name);
    if base_slug.is_empty() {
        return Err(super::bad_request(
            "Club name must contain at least one letter or number.",
        ));
    }
    let slug = {
        let mut candidate = base_slug.clone();
        let mut i = 1u32;
        loop {
            let c = candidate.clone();
            let exists = state
                .master_db
                .with_conn(move |conn| Tenant::find_by_slug(conn, &c))
                .await
                .map_err(super::internal_error)?;
            if exists.is_none() {
                break candidate;
            }
            i += 1;
            candidate = format!("{base_slug}-{i}");
        }
    };

    let now = Utc::now().naive_utc();
    let db_path = format!("{}/tenants/{slug}.db", state.data_dir);

    // Create tenant in master DB as grandfathered.
    let slug_clone = slug.clone();
    let db_path_clone = db_path.clone();
    let club_name_clone = club_name.clone();
    let tenant = state
        .master_db
        .with_conn(move |conn| {
            Tenant::create(
                conn,
                NewTenant {
                    name: club_name_clone,
                    slug: slug_clone,
                    db_path: db_path_clone,
                    created_at: now,
                    billing_status: BillingStatus::Grandfathered,
                },
            )
        })
        .await
        .map_err(super::internal_error)?;

    // Open the tenant DB (runs migrations on the fresh file).
    let (db, _config) = state
        .tenant_db(tenant.id)
        .await
        .map_err(super::internal_error)?;

    // Create team + PD user inside the tenant DB.
    db.with_conn(move |conn| {
        let _team = Team::create(
            conn,
            NewTeam {
                name: club_name,
                created_at: now,
            },
        )?;

        let hash = bcrypt::hash(&password, bcrypt::DEFAULT_COST)
            .map_err(|e| diesel::result::Error::QueryBuilderError(Box::new(e)))?;

        let user = AppUser::create(
            conn,
            NewAppUser {
                email,
                password_hash: Some(hash),
                name: admin_name,
                status: UserStatus::Active,
                created_at: now,
                updated_at: now,
                first_name: None,
                last_name: None,
            },
        )?;
        AppUser::set_role(conn, user.id, Role::ProgramDirector)?;
        Ok(())
    })
    .await
    .map_err(super::internal_error)?;

    // Redirect back to the dashboard.
    Ok(Redirect::to("/su"))
}
