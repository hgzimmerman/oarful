//! `/team` — Coach+ hub with tabs: Roster, Attendance, Sync.

use axum::{http::HeaderMap, response::Html, Extension};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use lineup_db::app_user::Role;

use crate::handlers::{self, ErrorResponse};
use crate::state::TenantContext;
use crate::templates::layout::{tab_swap, tabbed_section, TabDef};

const TABS: &[TabDef] = &[
    TabDef {
        label: "Roster",
        url: "/team/roster",
        id: "roster",
        min_role: Role::Coach,
    },
    TabDef {
        label: "Attendance",
        url: "/team/attendance",
        id: "attendance",
        min_role: Role::Coach,
    },
    TabDef {
        label: "Sync",
        url: "/team/sync",
        id: "sync",
        min_role: Role::Coach,
    },
];
const TARGET: &str = "team-tab-content";

/// `GET /team` — render the default tab (Roster).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn index_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let tab_content = handlers::rowers::roster_content(&jar, &tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "roster", TARGET, Role::Coach, tab_content).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "roster", TARGET, Role::Coach, tab_content);
    Ok(handlers::maybe_page_authed("Team", page, hx, &tenant))
}

/// `GET /team/roster`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn roster_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let tab_content = handlers::rowers::roster_content(&jar, &tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "roster", TARGET, Role::Coach, tab_content).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "roster", TARGET, Role::Coach, tab_content);
    Ok(handlers::maybe_page_authed("Team", page, hx, &tenant))
}

/// `GET /team/sync`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn sync_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let tab_content = handlers::sync::sync_content(&jar, &tenant).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "sync", TARGET, Role::Coach, tab_content).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "sync", TARGET, Role::Coach, tab_content);
    Ok(handlers::maybe_page_authed("Team", page, hx, &tenant))
}

/// `GET /team/attendance`
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn attendance_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<AttendanceQuery>,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let tab_content = attendance_content(&jar, &tenant, &query).await?;

    if is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(TABS, "attendance", TARGET, Role::Coach, tab_content).into_string(),
        ));
    }
    let page = tabbed_section(TABS, "attendance", TARGET, Role::Coach, tab_content);
    Ok(handlers::maybe_page_authed("Team", page, hx, &tenant))
}

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct AttendanceQuery {
    #[serde(default)]
    pub(crate) show_past: Option<String>,
}

pub(crate) async fn attendance_content(
    jar: &CookieJar,
    tenant: &TenantContext,
    query: &AttendanceQuery,
) -> Result<maud::Markup, ErrorResponse> {
    use lineup_db::availability::Availability;
    use lineup_db::practice::{Practice, PracticeId};
    use lineup_db::rower::Rower;
    use lineup_db::team::TeamMembership;

    let team_id = handlers::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let show_past = query.show_past.as_deref() == Some("1");
    let today = chrono::Utc::now().date_naive();

    let (rowers, practices, avail_map, committed_ids) = tenant
        .db
        .with_conn(move |conn| {
            let team_rower_ids = TeamMembership::rower_ids_for_team(conn, team_id)?;
            let all_active = Rower::list_active(conn)?;
            let mut rowers: Vec<Rower> = all_active
                .into_iter()
                .filter(|r| team_rower_ids.contains(&r.id))
                .collect();
            rowers.sort_by_key(|a| a.display_name());

            let since = if show_past {
                today - chrono::Duration::days(365)
            } else {
                today
            };
            let practices = Practice::list_since(conn, team_id, since)?;
            let practice_ids: Vec<PracticeId> = practices.iter().map(|p| p.id).collect();
            let avail_map = Availability::map_for_practices(conn, &practice_ids)?;
            let committed_ids: std::collections::HashSet<PracticeId> =
                Practice::committed_ids(conn, team_id, &practice_ids)?
                    .into_iter()
                    .collect();

            Ok((rowers, practices, avail_map, committed_ids))
        })
        .await
        .map_err(handlers::internal_error)?;

    let columns: Vec<crate::templates::attendance::PracticeColumn> = practices
        .iter()
        .map(|p| crate::templates::attendance::PracticeColumn {
            id: p.id,
            date: p.date,
            committed: committed_ids.contains(&p.id),
        })
        .collect();

    let is_coach = tenant.claims.role().at_least(Role::Coach);

    Ok(crate::templates::attendance::grid_content(
        &rowers, &columns, &avail_map, show_past, today, is_coach,
    ))
}

/// True when the HTMX request targets the tab content div (tab click),
/// as opposed to `#content` (navbar navigation).
fn is_tab_swap(headers: &HeaderMap) -> bool {
    headers.get("HX-Target").and_then(|v| v.to_str().ok()) == Some(TARGET)
}

// =====================================================================
// Attendance toggle (Coach+ inline edit)
// =====================================================================

#[derive(Debug, serde::Deserialize)]
pub(crate) struct AttendanceToggleInput {
    rower_id: lineup_db::rower::types::RowerId,
    practice_id: lineup_db::practice::PracticeId,
    status: String,
}

/// `POST /team/attendance/toggle` — cycle a single availability cell.
///
/// Returns the replacement `<td>` element for HTMX `outerHTML` swap.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn attendance_toggle_handler(
    Extension(tenant): Extension<TenantContext>,
    axum::Form(input): axum::Form<AttendanceToggleInput>,
) -> Result<Html<String>, ErrorResponse> {
    handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;

    let rower_id = input.rower_id;
    let practice_id = input.practice_id;

    use lineup_db::availability::types::AvailabilityStatus;
    use lineup_db::availability::{Availability, NewAvailability};

    let new_status: Option<AvailabilityStatus> = match input.status.as_str() {
        "Yes" => Some(AvailabilityStatus::Yes),
        "No" => Some(AvailabilityStatus::No),
        "clear" => None,
        _ => return Err(handlers::bad_request("Invalid status.")),
    };

    tenant
        .db
        .with_conn(move |conn| match new_status {
            Some(status) => Availability::upsert(
                conn,
                NewAvailability {
                    rower_id,
                    practice_id,
                    status,
                },
            ),
            None => Availability::delete(conn, rower_id, practice_id),
        })
        .await
        .map_err(handlers::internal_error)?;

    let status_str = match &new_status {
        Some(s) => s.to_string(),
        None => "clear".to_string(),
    };
    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "availability.update",
        "availability",
        &format!("{rower_id}:{practice_id}"),
        Some(
            serde_json::json!({
                "status": status_str,
                "practice_id": practice_id.as_int(),
                "set_by": "coach"
            })
            .to_string(),
        ),
    );

    // Return the replacement cell.
    Ok(Html(
        crate::templates::attendance::editable_status_cell_markup(
            new_status.as_ref(),
            rower_id,
            practice_id,
        )
        .into_string(),
    ))
}
