//! `/club` — Coach+ hub with tabs: Roster, Fleet, Sync.

use axum::{
    http::{HeaderMap, StatusCode},
    response::Html,
    Extension,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use lineup_db::app_user::Role;

use crate::state::TenantContext;
use crate::templates::layout::{tab_swap, tabbed_section, TabDef};
use crate::handlers;

const TABS: &[TabDef] = &[
    TabDef { label: "Roster", url: "/club/roster", id: "roster" },
    TabDef { label: "Attendance", url: "/club/attendance", id: "attendance" },
    TabDef { label: "Fleet", url: "/club/fleet", id: "fleet" },
    TabDef { label: "Sync", url: "/club/sync", id: "sync" },
];
const TARGET: &str = "club-tab-content";

/// `GET /club` — render the default tab (Roster).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn index_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let tab_content = handlers::rowers::roster_content(&jar, &tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(tab_swap(TABS, "roster", TARGET, tab_content).into_string()));
    }
    let page = tabbed_section(TABS, "roster", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Club", page, hx, &tenant))
}

/// `GET /club/roster`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn roster_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let tab_content = handlers::rowers::roster_content(&jar, &tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(tab_swap(TABS, "roster", TARGET, tab_content).into_string()));
    }
    let page = tabbed_section(TABS, "roster", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Club", page, hx, &tenant))
}

/// `GET /club/fleet`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn fleet_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let tab_content = handlers::boats::fleet_content(&tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(tab_swap(TABS, "fleet", TARGET, tab_content).into_string()));
    }
    let page = tabbed_section(TABS, "fleet", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Club", page, hx, &tenant))
}

/// `GET /club/sync`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn sync_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let tab_content = handlers::sync::sync_content(&jar, &tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(tab_swap(TABS, "sync", TARGET, tab_content).into_string()));
    }
    let page = tabbed_section(TABS, "sync", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Club", page, hx, &tenant))
}

/// `GET /club/attendance`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn attendance_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<AttendanceQuery>,
) -> Result<Html<String>, StatusCode> {
    handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let tab_content = attendance_content(&jar, &tenant, &query).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(tab_swap(TABS, "attendance", TARGET, tab_content).into_string()));
    }
    let page = tabbed_section(TABS, "attendance", TARGET, tab_content);
    Ok(handlers::maybe_page_authed("Club", page, hx, &tenant))
}

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct AttendanceQuery {
    #[serde(default)]
    pub(crate) show_past: Option<String>,
}

async fn attendance_content(
    jar: &CookieJar,
    tenant: &TenantContext,
    query: &AttendanceQuery,
) -> Result<maud::Markup, StatusCode> {
    use lineup_db::availability::Availability;
    use lineup_db::practice::Practice;
    use lineup_db::rower::Rower;
    use lineup_db::team::TeamMembership;

    let team_id = handlers::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let show_past = query.show_past.as_deref() == Some("1");
    let today = chrono::Utc::now().date_naive();

    let (rowers, dates, avail_map, committed_dates) = tenant
        .db
        .with_conn(move |conn| {
            let team_rower_ids = TeamMembership::rower_ids_for_team(conn, team_id)?;
            let all_active = Rower::list_active(conn)?;
            let mut rowers: Vec<Rower> = all_active
                .into_iter()
                .filter(|r| team_rower_ids.contains(&r.id))
                .collect();
            rowers.sort_by(|a, b| a.name.cmp(&b.name));

            let since = if show_past {
                today - chrono::Duration::days(365)
            } else {
                today
            };
            let dates = Practice::list_dates_since(conn, team_id, since)?;
            let avail_map = Availability::map_for_team_dates(conn, team_id, &dates)?;
            let committed_dates: std::collections::HashSet<chrono::NaiveDate> =
                Practice::committed_dates(conn, team_id, &dates)?
                    .into_iter()
                    .collect();

            Ok((rowers, dates, avail_map, committed_dates))
        })
        .await
        .map_err(handlers::internal_error)?;

    Ok(crate::templates::attendance::grid_content(
        &rowers, &dates, &avail_map, &committed_dates, show_past, today,
    ))
}

/// True when the HTMX request targets the tab content div (tab click),
/// as opposed to `#content` (navbar navigation).
fn is_tab_swap(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Target")
        .and_then(|v| v.to_str().ok())
        == Some(TARGET)
}
