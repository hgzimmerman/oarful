//! `/admin` — PD+ hub with tabs: Users, Teams, Audit.

use axum::{
    http::{HeaderMap, StatusCode},
    response::Html,
    Extension,
};
use axum_htmx::HxRequest;
use lineup_db::app_user::Role;

use crate::state::TenantContext;
use crate::templates::layout::{tab_swap, tabbed_section, TabDef};
use crate::handlers;
use crate::handlers::audit::AuditQuery;

const TABS: &[TabDef] = &[
    TabDef { label: "Users", url: "/admin/users", id: "users" },
    TabDef { label: "Teams", url: "/admin/teams", id: "teams" },
    TabDef { label: "Roster", url: "/admin/roster", id: "roster" },
    TabDef { label: "Fleet", url: "/admin/fleet", id: "fleet" },
    TabDef { label: "Audit", url: "/admin/audit", id: "audit" },
];
const TARGET: &str = "admin-tab-content";

/// `GET /admin` — render the default tab (Users).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn index_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = handlers::users::users_content(&tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(tab_swap(TABS, "users", TARGET, tab_content).into_string()));
    }
    let page = tabbed_section(TABS, "users", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/users`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn users_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = handlers::users::users_content(&tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(tab_swap(TABS, "users", TARGET, tab_content).into_string()));
    }
    let page = tabbed_section(TABS, "users", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/teams`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn teams_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = handlers::teams::teams_content(&tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(tab_swap(TABS, "teams", TARGET, tab_content).into_string()));
    }
    let page = tabbed_section(TABS, "teams", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/roster`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn roster_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = handlers::teams::roster_matrix_content(&tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(tab_swap(TABS, "roster", TARGET, tab_content).into_string()));
    }
    let page = tabbed_section(TABS, "roster", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/fleet`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn fleet_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = handlers::teams::fleet_matrix_content(&tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(tab_swap(TABS, "fleet", TARGET, tab_content).into_string()));
    }
    let page = tabbed_section(TABS, "fleet", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

/// `GET /admin/audit`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn audit_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<AuditQuery>,
) -> Result<Html<String>, StatusCode> {
    handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let tab_content = handlers::audit::audit_content(&tenant, &query).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(tab_swap(TABS, "audit", TARGET, tab_content).into_string()));
    }
    let page = tabbed_section(TABS, "audit", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

fn is_tab_swap(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Target")
        .and_then(|v| v.to_str().ok())
        == Some(TARGET)
}
