//! Superuser admin panel: tenant list, billing management, impersonation.

use axum::{
    extract::{Path, Request, State},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use axum_extra::extract::CookieJar;
use lineup_db::team::Team;
use lineup_master_db::tenant::{BillingStatus, Tenant, TenantId};
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
        .with_conn(|conn| Tenant::list_all(conn))
        .await
        .map_err(super::internal_error)?;

    Ok(Html(
        templates::superuser::su_dashboard(&tenants).into_string(),
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct BillingInput {
    status: String,
}

/// `POST /su/billing/{id}` — update a tenant's billing status.
#[tracing::instrument(level = "info", skip_all, err)]
pub(crate) async fn billing_handler(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Form(input): Form<BillingInput>,
) -> Result<Html<String>, super::ErrorResponse> {
    let tenant_id = TenantId::new(id);
    let status = BillingStatus::from_str(&input.status);

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

/// `POST /su/impersonate/{id}` — enter a tenant as PD.
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

    // Open the tenant DB.
    let (db, _config) = state
        .tenant_db(tenant_id)
        .await
        .map_err(super::internal_error)?;

    // Find a PD user to impersonate, falling back to any active user.
    let (user_id, team_id) = db
        .with_conn(move |conn| {
            use diesel::prelude::*;
            use lineup_db::schema::{app_user, user_role};

            // Try PD first, then any active user.
            let pd = user_role::table
                .inner_join(app_user::table.on(app_user::id.eq(user_role::user_id)))
                .filter(user_role::role.eq("ProgramDirector"))
                .filter(app_user::status.eq("active"))
                .select(app_user::id)
                .first::<i32>(conn)
                .optional()?;

            let user_id = match pd {
                Some(id) => lineup_db::app_user::UserId::new(id),
                None => {
                    // Fall back to any active user.
                    let any = app_user::table
                        .filter(app_user::status.eq("active"))
                        .select(app_user::id)
                        .first::<i32>(conn)?;
                    lineup_db::app_user::UserId::new(any)
                }
            };

            let team = Team::first(conn)?;
            let team_id = team
                .map(|t| t.id)
                .unwrap_or(lineup_db::team::TeamId::new(1));

            Ok((user_id, team_id))
        })
        .await
        .map_err(super::internal_error)?;

    // Issue an impersonation JWT (superuser flag preserved).
    let jwt = state
        .jwt_keys
        .issue_superuser_impersonation(user_id, tenant_id, team_id)
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
