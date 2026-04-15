//! Team selector and management.

use std::collections::HashSet;

use axum::{
    extract::Path,
    http::StatusCode,
    response::Html,
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use lineup_db::app_user::Role;
use lineup_db::boat::Boat;
use lineup_db::boat::types::BoatId;
use lineup_db::rower::Rower;
use lineup_db::rower::types::RowerId;
use lineup_db::team::{Team, TeamBoatDefault, TeamId, TeamMembership};
use serde::Deserialize;

use crate::{handlers::internal_error, state::TenantContext, templates};

/// `GET /teams/selector` — returns the team dropdown markup. Called
/// via `hx-trigger="load"` from the navbar placeholder so the layout
/// template stays a pure function.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn selector_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
) -> Result<Html<String>, StatusCode> {
    let active = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let role = tenant.claims.role().unwrap_or(Role::Member);
    let is_pd = role.at_least(Role::ProgramDirector);
    let is_coach = role.at_least(Role::Coach);

    let user_id = tenant.claims.user_id().as_int();
    let teams = tenant
        .db
        .with_conn(move |conn| {
            if is_pd {
                // PDs see all teams (including archived, so they can manage them).
                Team::list_all(conn)
            } else if is_coach {
                // Coaches see active teams they're assigned to.
                let team_ids = lineup_db::team::TeamMembership::team_ids_for_coach(conn, user_id)?;
                let active = Team::list_active(conn)?;
                Ok(active.into_iter().filter(|t| team_ids.contains(&t.id)).collect())
            } else {
                // Members see active teams their rower is in.
                use lineup_db::app_user::AppUser;
                let user = AppUser::get(conn, lineup_db::app_user::UserId::new(user_id))?;
                if let Some(rid) = user.and_then(|u| u.rower_id) {
                    let team_ids = lineup_db::team::TeamMembership::team_ids_for_rower(conn, rid)?;
                    let active = Team::list_active(conn)?;
                    Ok(active.into_iter().filter(|t| team_ids.contains(&t.id)).collect())
                } else {
                    // No linked rower — fall back to active (shouldn't normally happen).
                    Team::list_active(conn)
                }
            }
        })
        .await
        .map_err(internal_error)?;
    let tenant_name = if is_pd { Some(tenant.config.tenant_name.as_str()) } else { None };
    Ok(Html(
        templates::teams::selector(&teams, active, tenant_name).into_string(),
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateTeamInput {
    name: String,
}

/// `POST /teams` — create a new team (PD only).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn create_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<CreateTeamInput>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let now = chrono::Utc::now().naive_utc();
    let team = tenant
        .db
        .with_conn(move |conn| {
            Team::create(conn, lineup_db::team::NewTeam { name, created_at: now })
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "team.create",
        "team",
        &team.id.to_string(),
        Some(serde_json::json!({"name": team.name}).to_string()),
    );

    // Redirect to the new team's detail page.
    let content = templates::teams::detail_content(&team);
    Ok(super::maybe_page_authed(&format!("Team · {}", team.name), content, hx, &tenant))
}

/// Build the teams list markup (shared by `/teams` and `/admin/teams`).
pub(crate) async fn teams_content(
    tenant: &TenantContext,
) -> Result<maud::Markup, StatusCode> {
    let teams = tenant
        .db
        .with_conn(|conn| Team::list_all(conn))
        .await
        .map_err(internal_error)?;
    Ok(templates::teams::list_content(&teams))
}


/// `GET /teams/{id}` — team detail + config (PD only).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn detail_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<TeamId>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let team = tenant
        .db
        .with_conn(move |conn| Team::get(conn, id))
        .await
        .map_err(internal_error)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let content = templates::teams::detail_content(&team);
    Ok(super::maybe_page_authed(&format!("Team · {}", team.name), content, hx, &tenant))
}

#[derive(Debug, Deserialize)]
pub(crate) struct TeamUpdateInput {
    name: String,
    self_edit_level: String,
    #[serde(default)]
    default_practice_time: Option<String>,
}

/// `POST /teams/{id}` — update team config (PD only).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn update_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<TeamId>,
    hx: HxRequest,
    Form(input): Form<TeamUpdateInput>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let name = input.name.trim().to_string();
    let level = input.self_edit_level.clone();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let practice_time = input
        .default_practice_time
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| chrono::NaiveTime::parse_from_str(s, "%H:%M").ok());
    tenant
        .db
        .with_conn(move |conn| {
            use diesel::prelude::*;
            use lineup_db::schema::team;
            diesel::update(team::table.find(id))
                .set((
                    team::name.eq(&name),
                    team::self_edit_level.eq(&level),
                    team::default_practice_time.eq(practice_time),
                ))
                .execute(conn)
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "team.update",
        "team",
        &id.to_string(),
        None,
    );

    // Re-load and re-render.
    let team = tenant
        .db
        .with_conn(move |conn| Team::get(conn, id))
        .await
        .map_err(internal_error)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let content = templates::teams::detail_content(&team);
    Ok(super::maybe_page_authed(&format!("Team · {}", team.name), content, hx, &tenant))
}

// =====================================================================
// Roster matrix — rowers × teams assignment grid
// =====================================================================

/// Build the roster assignment matrix markup.
pub(crate) async fn roster_matrix_content(
    tenant: &TenantContext,
) -> Result<maud::Markup, StatusCode> {
    let (rowers, teams, memberships) = tenant
        .db
        .with_conn(|conn| {
            let rowers = Rower::list_active(conn)?;
            let teams = Team::list_all(conn)?;
            let memberships = TeamMembership::all(conn)?;
            Ok((rowers, teams, memberships))
        })
        .await
        .map_err(internal_error)?;

    let member_set: HashSet<(TeamId, RowerId)> = memberships
        .iter()
        .map(|m| (m.team_id, m.rower_id))
        .collect();

    Ok(templates::teams::roster_matrix(&rowers, &teams, &member_set))
}

/// `POST /admin/roster` — batch save team membership assignments.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn roster_matrix_save_handler(
    Extension(tenant): Extension<TenantContext>,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    // Parse checkbox values. Form fields are named "m_{team_id}_{rower_id}"
    // — present means checked.
    let desired: HashSet<(TeamId, RowerId)> = form
        .keys()
        .filter_map(|key| {
            let rest = key.strip_prefix("m_")?;
            let (tid, rid) = rest.split_once('_')?;
            Some((tid.parse::<TeamId>().ok()?, rid.parse::<RowerId>().ok()?))
        })
        .collect();

    let user_id = tenant.claims.user_id().as_int();
    let (added, removed) = tenant
        .db
        .with_conn(move |conn| {
            let current: HashSet<(TeamId, RowerId)> = TeamMembership::all(conn)?
                .into_iter()
                .map(|m| (m.team_id, m.rower_id))
                .collect();

            let to_add: Vec<_> = desired.difference(&current).copied().collect();
            let to_remove: Vec<_> = current.difference(&desired).copied().collect();

            for (team_id, rower_id) in &to_add {
                TeamMembership::add(conn, *team_id, *rower_id)?;
            }
            for (team_id, rower_id) in &to_remove {
                TeamMembership::remove(conn, *team_id, *rower_id)?;
            }
            Ok((to_add.len(), to_remove.len()))
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(user_id),
        "team.roster.update",
        "roster",
        "all",
        Some(serde_json::json!({"added": added, "removed": removed}).to_string()),
    );

    // Re-render the matrix with a toast.
    let (rowers, teams, memberships) = tenant
        .db
        .with_conn(|conn| {
            let rowers = Rower::list_active(conn)?;
            let teams = Team::list_all(conn)?;
            let memberships = TeamMembership::all(conn)?;
            Ok((rowers, teams, memberships))
        })
        .await
        .map_err(internal_error)?;

    let member_set: HashSet<(TeamId, RowerId)> = memberships
        .iter()
        .map(|m| (m.team_id, m.rower_id))
        .collect();

    let msg = format!("Saved. {added} added, {removed} removed.");
    Ok(Html(
        templates::teams::roster_matrix_with_toast(&msg, &rowers, &teams, &member_set)
            .into_string(),
    ))
}

// =====================================================================
// Fleet matrix — boats × teams default selection
// =====================================================================

/// Build the fleet assignment matrix markup.
pub(crate) async fn fleet_matrix_content(
    tenant: &TenantContext,
) -> Result<maud::Markup, StatusCode> {
    let (boats, teams, defaults) = tenant
        .db
        .with_conn(|conn| {
            let boats = Boat::list_sweep(conn)?;
            let teams = Team::list_all(conn)?;
            let defaults = TeamBoatDefault::all(conn)?;
            Ok((boats, teams, defaults))
        })
        .await
        .map_err(internal_error)?;

    let default_set: HashSet<(TeamId, BoatId)> = defaults
        .iter()
        .map(|d| (d.team_id, d.boat_id))
        .collect();

    Ok(templates::teams::fleet_matrix(&boats, &teams, &default_set))
}

/// `POST /admin/fleet` — batch save team boat defaults.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn fleet_matrix_save_handler(
    Extension(tenant): Extension<TenantContext>,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    // Form fields named "b_{team_id}_{boat_id}" — present means checked.
    let desired: HashSet<(TeamId, BoatId)> = form
        .keys()
        .filter_map(|key| {
            let rest = key.strip_prefix("b_")?;
            let (tid, bid) = rest.split_once('_')?;
            Some((tid.parse::<TeamId>().ok()?, bid.parse::<BoatId>().ok()?))
        })
        .collect();

    let user_id = tenant.claims.user_id().as_int();
    let (added, removed) = tenant
        .db
        .with_conn(move |conn| {
            let current: HashSet<(TeamId, BoatId)> = TeamBoatDefault::all(conn)?
                .into_iter()
                .map(|d| (d.team_id, d.boat_id))
                .collect();

            let to_add: Vec<_> = desired.difference(&current).copied().collect();
            let to_remove: Vec<_> = current.difference(&desired).copied().collect();

            for (team_id, boat_id) in &to_add {
                TeamBoatDefault::add(conn, *team_id, *boat_id)?;
            }
            for (team_id, boat_id) in &to_remove {
                TeamBoatDefault::remove(conn, *team_id, *boat_id)?;
            }
            Ok((to_add.len(), to_remove.len()))
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(user_id),
        "team.fleet.update",
        "fleet",
        "all",
        Some(serde_json::json!({"added": added, "removed": removed}).to_string()),
    );

    let (boats, teams, defaults) = tenant
        .db
        .with_conn(|conn| {
            let boats = Boat::list_sweep(conn)?;
            let teams = Team::list_all(conn)?;
            let defaults = TeamBoatDefault::all(conn)?;
            Ok((boats, teams, defaults))
        })
        .await
        .map_err(internal_error)?;

    let default_set: HashSet<(TeamId, BoatId)> = defaults
        .iter()
        .map(|d| (d.team_id, d.boat_id))
        .collect();

    let msg = format!("Saved. {added} added, {removed} removed.");
    Ok(Html(
        templates::teams::fleet_matrix_with_toast(&msg, &boats, &teams, &default_set)
            .into_string(),
    ))
}

// =====================================================================
// Archive / unarchive a team (PD only)
// =====================================================================

/// `POST /teams/{id}/toggle-archive` — archive or unarchive a team.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn toggle_archive_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<TeamId>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let team = tenant
        .db
        .with_conn(move |conn| Team::get(conn, id))
        .await
        .map_err(internal_error)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let new_archived = !team.archived.as_bool();
    tenant
        .db
        .with_conn(move |conn| Team::set_archived(conn, id, new_archived))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        if new_archived { "team.archive" } else { "team.unarchive" },
        "team",
        &id.to_string(),
        None,
    );

    // Re-load and re-render the detail page.
    let team = tenant
        .db
        .with_conn(move |conn| Team::get(conn, id))
        .await
        .map_err(internal_error)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let content = templates::teams::detail_content(&team);
    Ok(super::maybe_page_authed(&format!("Team · {}", team.name), content, hx, &tenant))
}
