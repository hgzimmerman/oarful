//! Team selector and management.

use std::collections::HashSet;

use axum::{extract::Path, response::Html, Extension, Form};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use lineup_db::app_user::Role;
use lineup_db::boat::types::BoatId;
use lineup_db::boat::Boat;
use lineup_db::rower::types::RowerId;
use lineup_db::rower::Rower;
use lineup_db::team::{BucketVisibility, Team, TeamBoatDefault, TeamId, TeamMembership};
use serde::Deserialize;

use crate::{
    handlers::{bad_request, internal_error, not_found, ErrorResponse},
    state::TenantContext,
    templates,
};

/// `GET /teams/selector` — returns the team dropdown markup. Called
/// via `hx-trigger="load"` from the navbar placeholder so the layout
/// template stays a pure function.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn selector_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
) -> Result<Html<String>, ErrorResponse> {
    let active = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let role = tenant.claims.role();
    let is_pd = role.at_least(Role::ProgramDirector);
    let is_coach = role.at_least(Role::Coach);

    let user_id = tenant.claims.user_id();
    let teams = tenant
        .db
        .with_conn(move |conn| {
            if is_pd {
                // PDs see all teams (including archived, so they can manage them).
                Team::list_all(conn)
            } else if is_coach {
                // Coaches see active teams they're assigned to.
                if let Some(uid) = user_id {
                    let team_ids = lineup_db::team::TeamMembership::team_ids_for_coach(conn, uid)?;
                    let active = Team::list_active(conn)?;
                    Ok(active
                        .into_iter()
                        .filter(|t| team_ids.contains(&t.id))
                        .collect())
                } else {
                    // Superuser viewing as coach — show all active teams.
                    Team::list_active(conn)
                }
            } else {
                // Members see active teams their rower is in.
                use lineup_db::app_user::AppUser;
                if let Some(uid) = user_id {
                    let user = AppUser::get(conn, uid)?;
                    if let Some(rid) = user.and_then(|u| u.rower_id) {
                        let team_ids =
                            lineup_db::team::TeamMembership::team_ids_for_rower(conn, rid)?;
                        let active = Team::list_active(conn)?;
                        Ok(active
                            .into_iter()
                            .filter(|t| team_ids.contains(&t.id))
                            .collect())
                    } else {
                        // No linked rower — fall back to active.
                        Team::list_active(conn)
                    }
                } else {
                    // Superuser — fall back to active.
                    Team::list_active(conn)
                }
            }
        })
        .await
        .map_err(internal_error)?;
    let tenant_name = if is_pd {
        Some(tenant.config.tenant_name.as_str())
    } else {
        None
    };
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
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(bad_request("Invalid request."));
    }
    let now = chrono::Utc::now().naive_utc();
    let team = tenant
        .db
        .with_conn(move |conn| {
            Team::create(
                conn,
                lineup_db::team::NewTeam {
                    name,
                    created_at: now,
                },
            )
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "team.create",
        "team",
        &team.id.to_string(),
        Some(serde_json::json!({"name": team.name}).to_string()),
    );

    // Redirect to the new team's detail page.
    let team_id = team.id;
    let thresholds = tenant
        .db
        .with_conn(move |conn| lineup_db::team_threshold::TeamThreshold::for_team(conn, team_id))
        .await
        .map_err(internal_error)?;
    let content = templates::teams::detail_content(&team, &thresholds);
    Ok(super::maybe_page_authed(
        &format!("Team · {}", team.name),
        content,
        hx,
        &tenant,
    ))
}

/// Build the teams list markup (shared by `/teams` and `/admin/teams`).
pub(crate) async fn teams_content(tenant: &TenantContext) -> Result<maud::Markup, ErrorResponse> {
    let teams = tenant
        .db
        .with_conn(Team::list_all)
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
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let team = tenant
        .db
        .with_conn(move |conn| Team::get(conn, id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("Team not found."))?;
    let thresholds = tenant
        .db
        .with_conn(move |conn| lineup_db::team_threshold::TeamThreshold::for_team(conn, id))
        .await
        .map_err(internal_error)?;
    let content = templates::teams::detail_content(&team, &thresholds);
    Ok(super::maybe_page_authed(
        &format!("Team · {}", team.name),
        content,
        hx,
        &tenant,
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct TeamUpdateInput {
    name: String,
    bucket_visibility: BucketVisibility,
    #[serde(default)]
    member_raw_metrics: Option<String>,
    #[serde(default)]
    default_practice_time: Option<String>,
    #[serde(default)]
    default_practice_duration_minutes: Option<String>,
    #[serde(default)]
    day_mon: Option<String>,
    #[serde(default)]
    day_tue: Option<String>,
    #[serde(default)]
    day_wed: Option<String>,
    #[serde(default)]
    day_thu: Option<String>,
    #[serde(default)]
    day_fri: Option<String>,
    #[serde(default)]
    day_sat: Option<String>,
    #[serde(default)]
    day_sun: Option<String>,
    #[serde(default)]
    assume_available: Option<String>,
}

/// `POST /teams/{id}` — update team config (PD only).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn update_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<TeamId>,
    hx: HxRequest,
    Form(input): Form<TeamUpdateInput>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let name = input.name.trim().to_string();
    let bv = input.bucket_visibility;
    let mrm: i32 = if input.member_raw_metrics.is_some() {
        1
    } else {
        0
    };
    if name.is_empty() {
        return Err(bad_request("Invalid request."));
    }
    let practice_time = input
        .default_practice_time
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| chrono::NaiveTime::parse_from_str(s, "%H:%M").ok());
    let practice_duration: Option<lineup_db::types::DurationMinutes> = input
        .default_practice_duration_minutes
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<i32>().ok())
        .filter(|&m| m > 0)
        .map(lineup_db::types::DurationMinutes::new);
    let mut days: Vec<chrono::Weekday> = Vec::new();
    if input.day_mon.is_some() {
        days.push(chrono::Weekday::Mon);
    }
    if input.day_tue.is_some() {
        days.push(chrono::Weekday::Tue);
    }
    if input.day_wed.is_some() {
        days.push(chrono::Weekday::Wed);
    }
    if input.day_thu.is_some() {
        days.push(chrono::Weekday::Thu);
    }
    if input.day_fri.is_some() {
        days.push(chrono::Weekday::Fri);
    }
    if input.day_sat.is_some() {
        days.push(chrono::Weekday::Sat);
    }
    if input.day_sun.is_some() {
        days.push(chrono::Weekday::Sun);
    }
    let practice_days = if days.is_empty() {
        None
    } else {
        Some(lineup_db::team::PracticeDays::from_weekdays(&days))
    };
    let assume_avail: i32 = if input.assume_available.is_some() {
        1
    } else {
        0
    };
    tenant
        .db
        .with_conn(move |conn| {
            use diesel::prelude::*;
            use lineup_db::schema::team;
            diesel::update(team::table.find(id))
                .set((
                    team::name.eq(&name),
                    team::bucket_visibility.eq(bv),
                    team::member_raw_metrics.eq(mrm),
                    team::default_practice_time.eq(practice_time),
                    team::default_practice_duration_minutes.eq(practice_duration),
                    team::default_practice_days.eq(practice_days),
                    team::assume_available.eq(assume_avail),
                ))
                .execute(conn)
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
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
        .ok_or_else(|| not_found("Team not found."))?;
    let thresholds = tenant
        .db
        .with_conn(move |conn| lineup_db::team_threshold::TeamThreshold::for_team(conn, id))
        .await
        .map_err(internal_error)?;
    let content = templates::teams::detail_content(&team, &thresholds);
    Ok(super::maybe_page_authed(
        &format!("Team · {}", team.name),
        content,
        hx,
        &tenant,
    ))
}

// =====================================================================
// Roster matrix — rowers × teams assignment grid
// =====================================================================

/// Build the roster assignment matrix markup.
pub(crate) async fn roster_matrix_content(
    tenant: &TenantContext,
) -> Result<maud::Markup, ErrorResponse> {
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

    Ok(templates::teams::roster_matrix(
        &rowers,
        &teams,
        &member_set,
    ))
}

/// `POST /admin/roster` — batch save team membership assignments.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn roster_matrix_save_handler(
    Extension(tenant): Extension<TenantContext>,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Result<Html<String>, ErrorResponse> {
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
        tenant.claims.audit_user_id(),
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
) -> Result<maud::Markup, ErrorResponse> {
    let (boats, teams, defaults) = tenant
        .db
        .with_conn(|conn| {
            let boats = Boat::list_in_service(conn)?;
            let teams = Team::list_all(conn)?;
            let defaults = TeamBoatDefault::all(conn)?;
            Ok((boats, teams, defaults))
        })
        .await
        .map_err(internal_error)?;

    let default_set: HashSet<(TeamId, BoatId)> =
        defaults.iter().map(|d| (d.team_id, d.boat_id)).collect();

    Ok(templates::teams::fleet_matrix(&boats, &teams, &default_set))
}

/// `POST /admin/fleet` — batch save team boat defaults.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn fleet_matrix_save_handler(
    Extension(tenant): Extension<TenantContext>,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Result<Html<String>, ErrorResponse> {
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
        tenant.claims.audit_user_id(),
        "team.fleet.update",
        "fleet",
        "all",
        Some(serde_json::json!({"added": added, "removed": removed}).to_string()),
    );

    let (boats, teams, defaults) = tenant
        .db
        .with_conn(|conn| {
            let boats = Boat::list_in_service(conn)?;
            let teams = Team::list_all(conn)?;
            let defaults = TeamBoatDefault::all(conn)?;
            Ok((boats, teams, defaults))
        })
        .await
        .map_err(internal_error)?;

    let default_set: HashSet<(TeamId, BoatId)> =
        defaults.iter().map(|d| (d.team_id, d.boat_id)).collect();

    let msg = format!("Saved. {added} added, {removed} removed.");
    Ok(Html(
        templates::teams::fleet_matrix_with_toast(&msg, &boats, &teams, &default_set).into_string(),
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
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let team = tenant
        .db
        .with_conn(move |conn| Team::get(conn, id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("Team not found."))?;

    let new_archived = !team.archived.as_bool();
    tenant
        .db
        .with_conn(move |conn| Team::set_archived(conn, id, new_archived))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        if new_archived {
            "team.archive"
        } else {
            "team.unarchive"
        },
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
        .ok_or_else(|| not_found("Team not found."))?;
    let thresholds = tenant
        .db
        .with_conn(move |conn| lineup_db::team_threshold::TeamThreshold::for_team(conn, id))
        .await
        .map_err(internal_error)?;
    let content = templates::teams::detail_content(&team, &thresholds);
    Ok(super::maybe_page_authed(
        &format!("Team · {}", team.name),
        content,
        hx,
        &tenant,
    ))
}

// =====================================================================
// Threshold config (PD only)
// =====================================================================

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ThresholdInput {
    metric: String,
    low_mid: f64,
    mid_high: f64,
    high_very: f64,
    #[serde(default)]
    erg_distance_m: Option<i32>,
}

/// `POST /teams/{id}/thresholds` — save thresholds + batch recalculate.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn threshold_save_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<TeamId>,
    axum::Json(input): axum::Json<ThresholdInput>,
) -> Result<Html<String>, ErrorResponse> {
    use lineup_db::team_threshold::{self, TeamThreshold};

    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let metric = input.metric.clone();
    if ![
        team_threshold::METRIC_WEIGHT,
        team_threshold::METRIC_HEIGHT,
        team_threshold::METRIC_STRENGTH,
    ]
    .contains(&metric.as_str())
    {
        return Err(bad_request("Invalid metric."));
    }
    // Strength thresholds are stored descending (low_mid=slow > high_very=fast).
    let valid_order = if metric == lineup_db::team_threshold::METRIC_STRENGTH {
        input.low_mid > input.mid_high && input.mid_high > input.high_very
    } else {
        input.low_mid < input.mid_high && input.mid_high < input.high_very
    };
    if !valid_order {
        return Err(bad_request("Invalid threshold order."));
    }

    // Convert display units to storage units.
    let (low_mid, mid_high, high_very) = match metric.as_str() {
        "weight" => (
            input.low_mid / 2.20462, // lbs → kg
            input.mid_high / 2.20462,
            input.high_very / 2.20462,
        ),
        "height" => (
            input.low_mid * 0.0254, // inches → metres
            input.mid_high * 0.0254,
            input.high_very * 0.0254,
        ),
        "strength" => (
            input.low_mid * 100.0, // seconds → centiseconds
            input.mid_high * 100.0,
            input.high_very * 100.0,
        ),
        _ => (input.low_mid, input.mid_high, input.high_very),
    };

    let row = TeamThreshold {
        team_id: id,
        metric: metric.clone(),
        low_mid,
        mid_high,
        high_very,
    };
    let erg_distance = input.erg_distance_m;

    let updated = tenant
        .db
        .with_conn(move |conn| {
            TeamThreshold::upsert(conn, &row)?;
            if metric == team_threshold::METRIC_STRENGTH {
                if let Some(dist) = erg_distance {
                    use diesel::prelude::*;
                    use lineup_db::schema::team;
                    diesel::update(team::table.find(id))
                        .set(team::erg_threshold_distance_m.eq(Some(dist)))
                        .execute(conn)?;
                }
            }
            let all = TeamThreshold::for_team(conn, id)?;
            let team = Team::get(conn, id)?;
            let erg_dist = team.and_then(|t| t.erg_threshold_distance_m);
            // Note: batch_derive writes global rower fields from per-team
            // thresholds. Multi-team rowers get last-save-wins semantics.
            team_threshold::batch_derive(conn, id, &all, erg_dist)
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "team.threshold.update",
        "team",
        &id.to_string(),
        Some(serde_json::json!({"metric": input.metric, "rowers_updated": updated}).to_string()),
    );

    Ok(Html(format!(
        r#"<div class="text-sm text-emerald-700 bg-emerald-50 border border-emerald-200 rounded px-3 py-2 mt-2">{updated} rower(s) updated.</div>"#
    )))
}

/// `GET /teams/{id}/histogram?metric=weight` — histogram data for slider.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn histogram_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<TeamId>,
    axum::extract::Query(q): axum::extract::Query<HistogramQuery>,
) -> Result<axum::Json<Vec<HistogramBin>>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let metric = q.metric.clone();
    let q_dist = q.dist;
    let bins = tenant
        .db
        .with_conn(move |conn| {
            use diesel::prelude::*;
            use lineup_db::rower::Rower;
            let rower_ids = TeamMembership::rower_ids_for_team(conn, id)?;
            let rowers: Vec<Rower> = lineup_db::schema::rower::table
                .filter(lineup_db::schema::rower::id.eq_any(&rower_ids))
                .filter(lineup_db::schema::rower::active.eq(1))
                .select(Rower::as_select())
                .get_results(conn)?;

            let values: Vec<f64> = match metric.as_str() {
                "weight" => rowers
                    .iter()
                    .filter_map(|r| r.weight_kg.map(|w| w.to_lbs()))
                    .collect(),
                "height" => rowers
                    .iter()
                    .filter_map(|r| r.height_m.map(|m| m.to_inches()))
                    .collect(),
                "strength" => {
                    let dist = q_dist.or_else(|| {
                        Team::get(conn, id)
                            .ok()
                            .flatten()
                            .and_then(|t| t.erg_threshold_distance_m)
                    });
                    if let Some(dist) = dist {
                        let mut splits = Vec::new();
                        for r in &rowers {
                            let tests = lineup_db::erg_test::ErgTest::list_for_rower(conn, r.id)?;
                            if let Some(t) = tests.iter().find(|t| t.distance_m == dist) {
                                splits.push(
                                    (t.time_cs as f64) / (t.distance_m as f64 / 500.0) / 100.0,
                                );
                            }
                        }
                        splits
                    } else {
                        Vec::new()
                    }
                }
                _ => Vec::new(),
            };
            Ok(build_histogram(&values, &metric))
        })
        .await
        .map_err(internal_error)?;

    Ok(axum::Json(bins))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct HistogramQuery {
    metric: String,
    #[serde(default)]
    dist: Option<i32>,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct HistogramBin {
    pub min: f64,
    pub max: f64,
    pub count: usize,
}

fn build_histogram(values: &[f64], metric: &str) -> Vec<HistogramBin> {
    if values.is_empty() {
        return Vec::new();
    }
    let bin_width: f64 = match metric {
        "weight" => 5.0,
        "height" => 2.0,
        "strength" => 2.0,
        _ => 5.0,
    };
    let min_val = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_val = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let bin_start = (min_val / bin_width).floor() * bin_width;
    let bin_end = ((max_val / bin_width).ceil() + 1.0) * bin_width;
    let mut bins = Vec::new();
    let mut edge = bin_start;
    while edge < bin_end {
        let next = edge + bin_width;
        let count = values.iter().filter(|&&v| v >= edge && v < next).count();
        bins.push(HistogramBin {
            min: edge,
            max: next,
            count,
        });
        edge = next;
    }
    bins
}
