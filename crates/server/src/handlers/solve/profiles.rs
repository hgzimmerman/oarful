//! Solver profile handlers: save, delete, preset bar.

use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    Extension,
};
use axum_extra::extract::{CookieJar, Query};
use chrono::NaiveDate;
use lineup_solver::SolverConfig;
use lineup_db::app_user::Role;

use crate::handlers::internal_error;
use crate::templates;

use super::*;

/// `GET /solve/{date}/preset-bar` — HTMX partial returning just the
/// preset selector bar with updated active state.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn preset_bar_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(date): Path<NaiveDate>,
    Query(knobs): Query<SolveKnobs>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let custom_profiles = tenant
        .db
        .with_conn(move |conn| {
            lineup_db::solver_profile::SolverProfile::list_for_team(conn, team_id)
        })
        .await
        .map_err(internal_error)?;

    let profile_names: Vec<(String, Option<String>)> = custom_profiles
        .iter()
        .map(|p| (p.name.clone(), p.description.clone()))
        .collect();

    Ok(Html(
        templates::solve::preset_bar(date, &knobs, &profile_names).into_string(),
    ))
}

/// `POST /solver-profile` — save the current preset as a custom profile.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn save_profile_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    axum::Form(input): axum::Form<SaveProfileInput>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let name = input.name.trim().to_string();
    if name.is_empty() || SolverConfig::is_builtin(&name) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Resolve the config from the preset (built-in or default).
    let config = SolverConfig::from_preset(&input.preset).unwrap_or_default();

    let description = input.description.filter(|d| !d.trim().is_empty());
    let new_profile = lineup_db::solver_profile::NewSolverProfile {
        team_id,
        name,
        description,
        skill_variance_weight: config.skill_variance_weight,
        pair_affinity_weight: config.pair_affinity_weight,
        seat_affinity_weight: config.seat_affinity_weight,
        side_preference_weight: config.side_preference_weight,
        weight_class_slack_weight: config.weight_class_slack_weight,
        cox_cooldown_penalty: config.cox_cooldown_penalty,
        placement_reward_weight: config.placement_reward_weight,
        pair_strength_weight: config.pair_strength_weight,
        bow_pair_strength_weight: config.bow_pair_strength_weight,
        height_balance_weight: config.height_balance_weight,
        end_pair_skill_weight: config.end_pair_skill_weight,
        engine_room_strength_weight: config.engine_room_strength_weight,
        partial_fill_bonus: config.partial_fill_bonus,
        non_scull_retention_weight: config.non_scull_retention_weight,
        bow_cox_fit_weight: config.bow_cox_fit_weight,
        top_boat_stacking_weight: config.top_boat_stacking_weight,
        pair_eligibility_weight: config.pair_eligibility_weight,
        minimize_bench_weight: config.minimize_bench_weight,
        boat_size_stacking_weight: config.boat_size_stacking_weight,
    };

    tenant
        .db
        .with_conn(move |conn| {
            lineup_db::solver_profile::SolverProfile::upsert(conn, new_profile)
        })
        .await
        .map_err(internal_error)?;

    // Redirect back to the referring page (or practices).
    Ok(Redirect::to("/practices"))
}

/// `DELETE /solver-profile/{name}` — delete a custom profile.
/// Built-in presets cannot be deleted. Returns 200 on success
/// (HTMX reloads the page).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn delete_profile_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    if name.is_empty() || SolverConfig::is_builtin(&name) {
        return Err(StatusCode::BAD_REQUEST);
    }

    tenant
        .db
        .with_conn(move |conn| {
            use lineup_db::solver_profile::SolverProfile;
            if let Some(profile) = SolverProfile::find_by_name(conn, team_id, &name)? {
                SolverProfile::delete(conn, profile.id)?;
            }
            Ok(())
        })
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::OK)
}
