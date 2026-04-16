//! Solver profile handlers: save, delete, preset bar.

use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    Extension,
};
use axum_extra::extract::{CookieJar, Query};
use lineup_db::app_user::Role;
use lineup_db::practice::PracticeId;
use lineup_solver::SolverConfig;

use crate::handlers::{bad_request, internal_error, ErrorResponse};
use crate::templates;

use super::*;

/// `GET /solve/{date}/preset-bar` — HTMX partial returning just the
/// preset selector bar with updated active state.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn preset_bar_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(practice_id): Path<PracticeId>,
    Query(knobs): Query<SolveKnobs>,
    hx: axum_htmx::HxRequest,
) -> Result<impl IntoResponse, ErrorResponse> {
    // Redirect to the solve page if accessed directly (not via HTMX).
    if !hx.0 {
        return Ok(Redirect::to(&format!("/solve/{practice_id}")).into_response());
    }
    let result = preset_bar_inner(jar, &tenant, practice_id, &knobs).await?;
    Ok(result.into_response())
}

async fn preset_bar_inner(
    jar: CookieJar,
    tenant: &crate::state::TenantContext,
    practice_id: PracticeId,
    knobs: &SolveKnobs,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let practice = tenant
        .db
        .with_conn(move |conn| {
            lineup_db::practice::Practice::get(conn, practice_id)?
                .ok_or(diesel::result::Error::NotFound)
        })
        .await
        .map_err(internal_error)?;
    let _date = practice.date;

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
        templates::solve::preset_bar(practice_id, &knobs, &profile_names).into_string(),
    ))
}

/// `POST /solver-profile` — save the current preset as a custom profile.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn save_profile_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    axum::Form(input): axum::Form<SaveProfileInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let name = input.name.trim().to_string();
    if name.is_empty() || SolverConfig::is_builtin(&name) {
        return Err(bad_request("Invalid profile name."));
    }
    let audit_name = name.clone();

    // Resolve the basis config, then overlay any form-supplied values.
    let basis = SolverConfig::from_preset(&input.preset).unwrap_or_default();

    let description = input.description.filter(|d| !d.trim().is_empty());
    let new_profile = lineup_db::solver_profile::NewSolverProfile {
        team_id,
        name,
        description,
        skill_variance_weight: input
            .skill_variance_weight
            .unwrap_or(basis.skill_variance_weight),
        pair_affinity_weight: input
            .pair_affinity_weight
            .unwrap_or(basis.pair_affinity_weight),
        seat_affinity_weight: input
            .seat_affinity_weight
            .unwrap_or(basis.seat_affinity_weight),
        side_preference_weight: input
            .side_preference_weight
            .unwrap_or(basis.side_preference_weight),
        weight_class_slack_weight: input
            .weight_class_slack_weight
            .unwrap_or(basis.weight_class_slack_weight),
        cox_cooldown_penalty: input
            .cox_cooldown_penalty
            .unwrap_or(basis.cox_cooldown_penalty),
        placement_reward_weight: input
            .placement_reward_weight
            .unwrap_or(basis.placement_reward_weight),
        pair_strength_weight: input
            .pair_strength_weight
            .unwrap_or(basis.pair_strength_weight),
        bow_pair_strength_weight: input
            .bow_pair_strength_weight
            .unwrap_or(basis.bow_pair_strength_weight),
        height_balance_weight: input
            .height_balance_weight
            .unwrap_or(basis.height_balance_weight),
        end_pair_skill_weight: input
            .end_pair_skill_weight
            .unwrap_or(basis.end_pair_skill_weight),
        engine_room_strength_weight: input
            .engine_room_strength_weight
            .unwrap_or(basis.engine_room_strength_weight),
        partial_fill_bonus: input.partial_fill_bonus.unwrap_or(basis.partial_fill_bonus),
        non_scull_retention_weight: input
            .non_scull_retention_weight
            .unwrap_or(basis.non_scull_retention_weight),
        bow_cox_fit_weight: input.bow_cox_fit_weight.unwrap_or(basis.bow_cox_fit_weight),
        top_boat_stacking_weight: input
            .top_boat_stacking_weight
            .unwrap_or(basis.top_boat_stacking_weight),
        pair_eligibility_weight: input
            .pair_eligibility_weight
            .unwrap_or(basis.pair_eligibility_weight),
        minimize_bench_weight: input
            .minimize_bench_weight
            .unwrap_or(basis.minimize_bench_weight),
        boat_size_stacking_weight: input
            .boat_size_stacking_weight
            .unwrap_or(basis.boat_size_stacking_weight),
        bench_cooldown_penalty: input
            .bench_cooldown_penalty
            .unwrap_or(basis.bench_cooldown_penalty),
        stroke_spread_weight: input
            .stroke_spread_weight
            .unwrap_or(basis.stroke_spread_weight),
        eight_bias: input.eight_bias.unwrap_or(basis.eight_bias),
        coxed_four_bias: input.coxed_four_bias.unwrap_or(basis.coxed_four_bias),
        four_bias: input.four_bias.unwrap_or(basis.four_bias),
        quad_bias: input.quad_bias.unwrap_or(basis.quad_bias),
        pair_bias: input.pair_bias.unwrap_or(basis.pair_bias),
        double_bias: input.double_bias.unwrap_or(basis.double_bias),
        single_bias: input.single_bias.unwrap_or(basis.single_bias),
    };

    tenant
        .db
        .with_conn(move |conn| lineup_db::solver_profile::SolverProfile::upsert(conn, new_profile))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "solver_profile.save",
        "solver_profile",
        &audit_name,
        None,
    );

    // Redirect back to the referring page (or practices).
    Ok(Redirect::to("/practices"))
}

/// `GET /solver-profile/edit` — return the profile editor modal as an
/// HTMX partial. Query params:
/// - `name` — profile name to edit (empty = new)
/// - `basis` — basis preset name (for new/duplicate)
#[derive(Debug, serde::Deserialize)]
pub(crate) struct EditProfileQuery {
    #[serde(default)]
    name: String,
    #[serde(default = "default_basis")]
    basis: String,
}
fn default_basis() -> String {
    "balanced".to_string()
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn edit_profile_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Query(query): Query<EditProfileQuery>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let name = query.name.trim().to_string();
    let basis = query.basis.trim().to_string();
    let is_builtin = SolverConfig::is_builtin(&name)
        || (!name.is_empty() && SolverConfig::from_preset(&name).is_some());

    // Resolve the config to display.
    let (config, description, display_name, basis_name) = if is_builtin {
        // Viewing a built-in preset (read-only).
        let cfg = SolverConfig::from_preset(&name).unwrap_or_default();
        (cfg, None, name.clone(), name.clone())
    } else if !name.is_empty() {
        // Editing an existing custom profile.
        let name_clone = name.clone();
        let profile = tenant
            .db
            .with_conn(move |conn| {
                lineup_db::solver_profile::SolverProfile::find_by_name(conn, team_id, &name_clone)
            })
            .await
            .map_err(internal_error)?;
        match profile {
            Some(p) => {
                let cfg = profile_to_config(&p);
                (cfg, p.description.clone(), name.clone(), basis.clone())
            }
            None => {
                // Profile not found — treat as new with basis.
                let cfg = SolverConfig::from_preset(&basis).unwrap_or_default();
                (cfg, None, String::new(), basis.clone())
            }
        }
    } else {
        // Creating a new profile from a basis.
        let cfg = SolverConfig::from_preset(&basis).unwrap_or_default();
        (cfg, None, String::new(), basis.clone())
    };

    Ok(Html(
        templates::solve::profile_modal::profile_editor_modal(
            &display_name,
            &basis_name,
            &config,
            description.as_deref(),
            is_builtin,
        )
        .into_string(),
    ))
}

/// `DELETE /solver-profile/{name}` — delete a custom profile.
/// Built-in presets cannot be deleted. Returns 200 on success
/// (HTMX reloads the page).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn delete_profile_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(name): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    if name.is_empty() || SolverConfig::is_builtin(&name) {
        return Err(bad_request("Invalid profile name."));
    }

    let profile_name = name.clone();
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

    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "solver_profile.delete",
        "solver_profile",
        &profile_name,
        None,
    );

    Ok(StatusCode::OK)
}
