//! `GET /solve/{id}` — main solve view handler.

use axum::{extract::Path, response::Html, Extension};
use axum_extra::extract::{CookieJar, Query};
use axum_htmx::HxRequest;
use lineup_db::app_user::Role;
use lineup_db::practice::{Practice, PracticeId};
use lineup_db::snapshot::DbSnapshot;

use crate::templates;

use super::*;

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn view_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(practice_id): Path<PracticeId>,
    Query(knobs): Query<SolveKnobs>,
    hx: HxRequest,
) -> Result<Html<String>, super::ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let (practice, mut snapshot, committed_practices, has_committed) = tenant
        .db
        .with_conn(move |conn| {
            let practice =
                Practice::get(conn, practice_id)?.ok_or(diesel::result::Error::NotFound)?;
            let snapshot = DbSnapshot::for_practice(conn, &practice)?;
            let practices = Practice::list_committed(conn, team_id)?;
            let has_committed = {
                use lineup_db::lineup::Lineup;
                Lineup::for_practice(conn, practice.id)
                    .map(|l| !l.is_empty())
                    .unwrap_or(false)
            };
            Ok((practice, snapshot, practices, has_committed))
        })
        .await
        .map_err(internal_error)?;

    let date = practice.date;

    // Apply walk-on overrides before anything reads availability.
    apply_walkons(&mut snapshot, &knobs);

    // Load custom solver profiles for this team.
    let custom_profiles = tenant
        .db
        .with_conn(move |conn| {
            lineup_db::solver_profile::SolverProfile::list_for_team(conn, team_id)
        })
        .await
        .map_err(internal_error)?;

    // Apply no-shows before anything reads availability — affects
    // both the editor pool and the solver.
    apply_no_shows(&mut snapshot, &knobs);

    // When generate=1, return the streaming skeleton. For HTMX
    // requests (form submit), swap just #solve-results. For direct
    // navigation (browser URL), wrap in the full page with knobs.
    if knobs.generate > 0 {
        let skeleton = templates::solve::streaming_skeleton(practice_id, &knobs);
        if hx.0 {
            return Ok(Html(skeleton.into_string()));
        }
        let profile_names: Vec<(String, Option<String>)> = custom_profiles
            .iter()
            .map(|p| (p.name.clone(), p.description.clone()))
            .collect();
        let content = templates::solve::streaming_page(
            &snapshot,
            practice_id,
            date,
            &knobs,
            &committed_practices,
            &profile_names,
        );
        return Ok(crate::handlers::maybe_page_authed(
            &format!("Set Lineups · {date}"),
            content,
            hx,
            &tenant,
        ));
    }

    // Load team boat defaults. For single-team tenants with no
    // defaults configured, all boats remain active (empty set).
    let default_boats: std::collections::HashSet<lineup_db::boat::types::BoatId> = {
        let team_count = tenant
            .db
            .with_conn(|conn| lineup_db::team::Team::list_all(conn).map(|t| t.len()))
            .await
            .map_err(internal_error)?;
        let defaults = tenant
            .db
            .with_conn(move |conn| {
                lineup_db::team::TeamBoatDefault::boat_ids_for_team(conn, team_id)
            })
            .await
            .map_err(internal_error)?;
        // Single-team tenant with no defaults → all boats (empty set signals "all").
        // Multi-team tenant: use whatever is configured (even if empty → none pre-selected).
        if team_count <= 1 && defaults.is_empty() {
            std::collections::HashSet::new()
        } else {
            defaults.into_iter().collect()
        }
    };

    // Landing page: show knobs + "Generate" / "Re-generate" button.
    let profile_names: Vec<(String, Option<String>)> = custom_profiles
        .iter()
        .map(|p| (p.name.clone(), p.description.clone()))
        .collect();
    let flags = templates::solve::DisplayFlags {
        show_attributes: tenant.show_attributes(),
        force_cox_stern: tenant.config.force_cox_stern,
        locked_seats: SolveKnobs::parse_triples(&knobs.lock),
        pinned_seats: SolveKnobs::parse_triples(&knobs.pin),
        was_pinned_seats: SolveKnobs::parse_triples(&knobs.was_pin),
        pinned_boats: SolveKnobs::parse_boat_ids(&knobs.boat_pin),
        was_pinned_boats: SolveKnobs::parse_boat_ids(&knobs.boat_was_pin),
        locked_boats: SolveKnobs::parse_boat_ids(&knobs.boat_lock),
        boats_in_use_by: std::collections::HashMap::new(),
    };
    let content = templates::solve::landing_content(
        &snapshot,
        practice_id,
        date,
        &knobs,
        &committed_practices,
        has_committed,
        &profile_names,
        &flags,
        &default_boats,
    );
    Ok(crate::handlers::maybe_page_authed(
        &format!("Set Lineups · {date}"),
        content,
        hx,
        &tenant,
    ))
}
