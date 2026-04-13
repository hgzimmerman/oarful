//! Demo mode: ephemeral tenants with pre-seeded fixture data.
//!
//! - `POST /demo` — create a demo tenant, seed it, issue JWT, redirect
//! - Cleanup: `cleanup_expired_demos` deletes stale tenants + DB files

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use lineup_db::app_user::{AppUser, Role};
use lineup_master_db::tenant::{NewTenant, Tenant};

use crate::state::AppState;

/// Cookie name for resuming a demo session after logout.
pub(crate) const DEMO_SLUG_COOKIE: &str = "demo_slug";

/// Demo tenant lifetime.
const DEMO_DAYS: i64 = 7;

/// Max-age for the demo_slug cookie (matches tenant lifetime).
const DEMO_SLUG_MAX_AGE: time::Duration = time::Duration::days(DEMO_DAYS);

/// `POST /demo` — create an ephemeral demo tenant.
#[tracing::instrument(level = "info", skip_all, err)]
pub(crate) async fn create_demo_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<impl IntoResponse, StatusCode> {
    // Soft rate limit: if the user already has a demo_slug cookie,
    // redirect to resume instead of creating a new one.
    if jar.get(DEMO_SLUG_COOKIE).is_some() {
        return Ok(Redirect::to("/demo/resume").into_response());
    }

    let now = Utc::now().naive_utc();
    let expires = now + chrono::TimeDelta::try_days(DEMO_DAYS).unwrap();

    // Generate a random slug.
    let slug = {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        let h = RandomState::new().build_hasher().finish();
        format!("demo-{h:012x}")
    };
    let db_path = format!("{}/demos/{slug}.db", state.data_dir);

    // Create the tenant in the master DB.
    let slug_clone = slug.clone();
    let db_path_clone = db_path.clone();
    let tenant = state
        .master_db
        .with_conn(move |conn| {
            Tenant::create(
                conn,
                NewTenant {
                    name: format!("Demo Club ({slug_clone})"),
                    slug: slug_clone,
                    db_path: db_path_clone,
                    created_at: now,
                },
            )
        })
        .await
        .map_err(super::internal_error)?;

    // Set demo_expires_at on the tenant.
    let tenant_id = tenant.id;
    state
        .master_db
        .with_conn(move |conn| {
            use diesel::prelude::*;
            use lineup_master_db::schema::tenant as t;
            diesel::update(t::table.find(tenant_id))
                .set(t::demo_expires_at.eq(Some(expires)))
                .execute(conn)?;
            Ok(())
        })
        .await
        .map_err(super::internal_error)?;

    // Open the tenant DB (runs migrations on the fresh file).
    let (db, _config) = state
        .tenant_db(tenant.id)
        .await
        .map_err(super::internal_error)?;

    // Seed the demo fixture and get the demo user's ID.
    let demo_user_id = db
        .with_conn(|conn| lineup_db::fixture::seed_demo(conn))
        .await
        .map_err(super::internal_error)?;

    // Look up the demo user to get their role and default team.
    let (role, default_team) = db
        .with_conn(move |conn| {
            let role = AppUser::role(conn, demo_user_id)?;
            let team = lineup_db::team::Team::first(conn)?;
            Ok((role, team))
        })
        .await
        .map_err(super::internal_error)?;

    let role = role.unwrap_or(Role::Member);
    let team_id = default_team
        .map(|t| t.id)
        .unwrap_or(lineup_db::team::TeamId::new(1));

    // Issue a long-lived JWT (matches demo lifetime).
    let exp_unix = expires.and_utc().timestamp() as u64;
    let jwt = state
        .jwt_keys
        .issue_with_expiry(demo_user_id, tenant.id, role, team_id, exp_unix)
        .map_err(super::internal_error)?;

    let jwt_cookie =
        axum_extra::extract::cookie::Cookie::build((super::auth::TOKEN_COOKIE, jwt))
            .path("/")
            .http_only(true)
            .same_site(axum_extra::extract::cookie::SameSite::Lax);

    // Set demo_slug cookie so the user can resume after logout.
    let demo_cookie =
        axum_extra::extract::cookie::Cookie::build((DEMO_SLUG_COOKIE, slug))
            .path("/")
            .http_only(true)
            .max_age(DEMO_SLUG_MAX_AGE)
            .same_site(axum_extra::extract::cookie::SameSite::Lax);

    let jar = jar.add(jwt_cookie).add(demo_cookie);
    Ok((jar, Redirect::to("/practices")).into_response())
}

/// Resume a demo session. Called from the login page when a
/// `demo_slug` cookie is present. Re-issues a JWT for the demo
/// tenant's coach user.
#[tracing::instrument(level = "info", skip_all, err)]
pub(crate) async fn resume_demo_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<impl IntoResponse, StatusCode> {
    let slug = jar
        .get(DEMO_SLUG_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let (tenant_id, db, _config) = state
        .tenant_db_by_slug(&slug)
        .await
        .map_err(super::internal_error)?;

    // Verify the tenant is still a live demo.
    let tenant = state
        .master_db
        .with_conn(move |conn| Tenant::get(conn, tenant_id))
        .await
        .map_err(super::internal_error)?
        .ok_or(StatusCode::NOT_FOUND)?;

    match tenant.demo_expires_at {
        Some(exp) if exp > Utc::now().naive_utc() => {}
        _ => {
            // Expired or not a demo — clear the cookie.
            let jar = jar.remove(
                axum_extra::extract::cookie::Cookie::build(DEMO_SLUG_COOKIE).path("/"),
            );
            return Ok((jar, Redirect::to("/login")).into_response());
        }
    }

    // Find the demo coach user.
    let user = db
        .with_conn(|conn| AppUser::find_by_email(conn, "demo@localhost"))
        .await
        .map_err(super::internal_error)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let (role, default_team) = db
        .with_conn(move |conn| {
            let role = AppUser::role(conn, user.id)?;
            let team = lineup_db::team::Team::first(conn)?;
            Ok((role, team))
        })
        .await
        .map_err(super::internal_error)?;

    let role = role.unwrap_or(Role::Member);
    let team_id = default_team
        .map(|t| t.id)
        .unwrap_or(lineup_db::team::TeamId::new(1));
    let exp_unix = tenant.demo_expires_at.unwrap().and_utc().timestamp() as u64;

    let jwt = state
        .jwt_keys
        .issue_with_expiry(user.id, tenant_id, role, team_id, exp_unix)
        .map_err(super::internal_error)?;

    let jwt_cookie =
        axum_extra::extract::cookie::Cookie::build((super::auth::TOKEN_COOKIE, jwt))
            .path("/")
            .http_only(true)
            .same_site(axum_extra::extract::cookie::SameSite::Lax);

    let jar = jar.add(jwt_cookie);
    Ok((jar, Redirect::to("/practices")).into_response())
}

/// Delete expired demo tenants and their SQLite files. Called at
/// startup and optionally on a timer.
pub async fn cleanup_expired_demos(state: &AppState) {
    let expired = match state
        .master_db
        .with_conn(|conn| Tenant::list_expired_demos(conn))
        .await
    {
        Ok(list) => list,
        Err(e) => {
            tracing::error!(?e, "failed to list expired demos");
            return;
        }
    };

    if expired.is_empty() {
        return;
    }

    tracing::info!(count = expired.len(), "cleaning up expired demo tenants");
    for t in &expired {
        // Remove the SQLite file.
        if let Err(e) = std::fs::remove_file(&t.db_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(db_path = %t.db_path, ?e, "failed to remove demo DB file");
            }
        }
        // Evict from the in-memory connection cache.
        state.evict_tenant(t.id);
        // Remove from master DB.
        let tid = t.id;
        if let Err(e) = state
            .master_db
            .with_conn(move |conn| Tenant::delete(conn, tid))
            .await
        {
            tracing::warn!(tenant_id = %t.id, ?e, "failed to delete demo tenant row");
        } else {
            tracing::info!(tenant_id = %t.id, slug = %t.slug, "deleted expired demo tenant");
        }
    }
}
