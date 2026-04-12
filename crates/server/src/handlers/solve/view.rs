//! `GET /solve/{date}` — main solve view handler.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Html,
    Extension,
};
use axum_extra::extract::{CookieJar, Query};
use axum_htmx::HxRequest;
use chrono::NaiveDate;
use lineup_db::snapshot::DbSnapshot;
use lineup_db::practice::Practice;
use lineup_db::app_user::Role;

use crate::{state::AppState, templates};

use super::*;

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn view_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(date): Path<NaiveDate>,
    Query(knobs): Query<SolveKnobs>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let (mut snapshot, committed_practices, has_committed) = tenant
        .db
        .with_conn(move |conn| {
            let snapshot = DbSnapshot::for_team_date(conn, team_id, date)?;
            let practices = Practice::list_committed(conn, team_id)?;
            let has_committed = Practice::find_by_date(conn, team_id, date)?
                .map(|p| {
                    use lineup_db::lineup::Lineup;
                    Lineup::for_practice(conn, p.id)
                        .map(|l| !l.is_empty())
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            Ok((snapshot, practices, has_committed))
        })
        .await
        .map_err(internal_error)?;

    // Apply walk-on overrides before anything reads availability.
    apply_walkons(&mut snapshot, &knobs);

    // Load custom solver profiles for this team (used by both the
    // resolver and the template's preset selector).
    let preset_name = knobs.preset.clone();
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

    // Only run the solver when explicitly requested via generate=1.
    if knobs.generate > 0 {
        let baselines = build_baselines(&knobs, &tenant.db, team_id).await?;

        let _permit = acquire_solve_permit(&state).await?;
        // Resolve config: check custom profiles first, then built-in presets.
        let config = custom_profiles
            .iter()
            .find(|p| p.name == preset_name)
            .map(|p| profile_to_config(p))
            .unwrap_or_else(|| knobs.resolve_config());
        let mut request = knobs.to_request(date, &snapshot, baselines);
        request.config = config;
        // Combine explicit locks + dirty pins as solver constraints.
        let mut solver_locks = knobs.parse_locks();
        for entry in &knobs.pin {
            let parts: Vec<&str> = entry.splitn(3, ':').collect();
            if parts.len() == 3 {
                if let (Ok(rid), Ok(bid), Ok(seat)) = (parts[0].parse(), parts[1].parse(), parts[2].parse()) {
                    solver_locks.push(lineup_solver::SeatLock { rower_id: rid, boat_id: bid, seat });
                }
            }
        }
        request.locks = solver_locks;
        let result = run_solve(&state, snapshot.clone(), request).await?;

        // State transitions: pin→was_pin, was_pin→dropped, lock→lock.
        // Pins reference a specific boat, but the solver may have moved
        // the rower to a different boat. Match pinned rowers against
        // their actual placement in the solver output.
        let locked_seats = SolveKnobs::parse_triples(&knobs.lock);
        let pinned_rowers: std::collections::HashSet<lineup_db::rower::types::RowerId> =
            knobs.pin.iter().filter_map(|e| {
                e.splitn(3, ':').next()?.parse().ok()
            }).collect();
        let was_pinned_seats: std::collections::HashSet<(lineup_db::rower::types::RowerId, lineup_db::boat::types::BoatId, i32)> =
            result.primary.lineups.iter()
                .filter(|l| l.used)
                .flat_map(|l| l.seats.iter().map(move |&(seat, rid)| (rid, l.boat_id, seat)))
                .filter(|(rid, _, _)| pinned_rowers.contains(rid))
                .collect();
        let pinned_seats = std::collections::HashSet::new(); // fresh solve clears dirty
        // Transform knobs for the response: pin→was_pin (at actual positions), was_pin→cleared.
        let mut response_knobs = knobs.clone();
        response_knobs.was_pin = was_pinned_seats.iter()
            .map(|(rid, bid, seat)| format!("{rid}:{bid}:{seat}"))
            .collect();
        response_knobs.pin = vec![];
        // Boat state transitions: boat_pin→boat_was_pin, boat_was_pin→dropped, boat_lock→boat_lock.
        let locked_boats = SolveKnobs::parse_boat_ids(&knobs.boat_lock);
        let was_pinned_boats = SolveKnobs::parse_boat_ids(&knobs.boat_pin);
        let pinned_boats = std::collections::HashSet::new();
        response_knobs.boat_was_pin = knobs.boat_pin.clone();
        response_knobs.boat_pin = vec![];
        let flags = templates::solve::DisplayFlags {
            show_attributes: tenant.show_attributes(),
            force_cox_stern: tenant.config.force_cox_stern,
            locked_seats,
            pinned_seats,
            was_pinned_seats,
            pinned_boats,
            was_pinned_boats,
            locked_boats,
        };
        let profile_names: Vec<(String, Option<String>)> = custom_profiles.iter().map(|p| (p.name.clone(), p.description.clone())).collect();
        let content = templates::solve::view_content(
            &snapshot, date, &response_knobs, &result, &committed_practices,
            &flags, &profile_names,
        );
        return Ok(crate::handlers::maybe_page_authed(
            &format!("Set Lineups · {date}"),
            content,
            hx,
            &tenant,
        ));
    }

    // Landing page: show knobs + "Generate" / "Re-generate" button.
    // If seat params are present (e.g. from "Edit lineup" on history),
    // pre-populate the editor with those placements.
    let profile_names: Vec<(String, Option<String>)> = custom_profiles.iter().map(|p| (p.name.clone(), p.description.clone())).collect();
    let flags = templates::solve::DisplayFlags {
        show_attributes: tenant.show_attributes(),
        force_cox_stern: tenant.config.force_cox_stern,
        locked_seats: SolveKnobs::parse_triples(&knobs.lock),
        pinned_seats: SolveKnobs::parse_triples(&knobs.pin),
        was_pinned_seats: SolveKnobs::parse_triples(&knobs.was_pin),
        pinned_boats: SolveKnobs::parse_boat_ids(&knobs.boat_pin),
        was_pinned_boats: SolveKnobs::parse_boat_ids(&knobs.boat_was_pin),
        locked_boats: SolveKnobs::parse_boat_ids(&knobs.boat_lock),
    };
    let content = templates::solve::landing_content(
        &snapshot, date, &knobs, &committed_practices, has_committed,
        &profile_names, &flags,
    );
    Ok(crate::handlers::maybe_page_authed(
        &format!("Set Lineups · {date}"),
        content,
        hx,
        &tenant,
    ))
}
