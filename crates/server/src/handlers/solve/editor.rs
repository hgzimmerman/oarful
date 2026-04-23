//! `GET /solve/{id}/editor` — re-render the lineup editor partial.

use axum::{extract::Path, response::Html, Extension};
use axum_extra::extract::{CookieJar, Query};
use lineup_db::app_user::Role;
use lineup_db::boat::types::BoatId;
use lineup_db::practice::{Practice, PracticeId};
use lineup_db::snapshot::DbSnapshot;

use crate::handlers::{internal_error, ErrorResponse};
use crate::templates;

use super::*;

/// `GET /solve/{id}/editor` — re-render the lineup editor from the
/// given placement state. No solver run — just snapshot lookup + template.
/// Used by the Alpine component after each client-side operation.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn editor_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(practice_id): Path<PracticeId>,
    Query(params): Query<EditorParams>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let (practice, mut snapshot) = tenant
        .db
        .with_conn(move |conn| {
            let practice =
                Practice::get(conn, practice_id)?.ok_or(diesel::result::Error::NotFound)?;
            let snapshot = DbSnapshot::for_practice(conn, &practice)?;
            Ok((practice, snapshot))
        })
        .await
        .map_err(internal_error)?;

    let _date = practice.date;

    // Load overlapping practices from other teams.
    let practice_for_overlap = practice.clone();
    let team_id_for_overlap = _team_id;
    let (other_team_rowers, boats_in_use) = tenant
        .db
        .with_conn(move |conn| {
            let team = lineup_db::team::Team::get(conn, team_id_for_overlap)?;
            let team_dur = team.and_then(|t| t.default_practice_duration_minutes);
            let overlapping = Practice::find_overlapping(conn, &practice_for_overlap, team_dur)?;

            let mut other_rowers: Vec<(lineup_db::rower::Rower, String)> = Vec::new();
            let mut boats_in_use: std::collections::HashMap<BoatId, String> =
                std::collections::HashMap::new();

            for op in &overlapping {
                let other_team = lineup_db::team::Team::get(conn, op.team_id)?;
                let other_assume = other_team
                    .as_ref()
                    .map(|t| t.assume_available.as_bool())
                    .unwrap_or(false);
                let team_name = other_team.map(|t| t.name.clone()).unwrap_or_default();

                // Find committed lineups for this overlapping practice — boats in use.
                let committed = lineup_db::lineup::Lineup::for_practice(conn, op.id)?;
                for cl in &committed {
                    boats_in_use
                        .entry(cl.lineup.boat_id)
                        .or_insert_with(|| team_name.clone());
                }

                // Find available rowers from this other practice.
                let other_team_rower_ids =
                    lineup_db::team::TeamMembership::rower_ids_for_team(conn, op.team_id)?;
                let other_avail =
                    lineup_db::availability::Availability::map_for_practice(conn, op.id)?;
                let placed_rower_ids: std::collections::HashSet<lineup_db::rower::types::RowerId> =
                    committed
                        .iter()
                        .flat_map(|cl| cl.seats.iter().map(|s| s.rower_id))
                        .collect();

                // Get rowers who are available but not placed in any lineup.
                for rid in &other_team_rower_ids {
                    if placed_rower_ids.contains(rid) {
                        continue;
                    }
                    if other_avail
                        .get(rid)
                        .map(|s| s.is_available())
                        .unwrap_or(other_assume)
                    {
                        if let Some(rower) = lineup_db::rower::Rower::get(conn, *rid)? {
                            if rower.active.as_bool() {
                                other_rowers.push((rower, team_name.clone()));
                            }
                        }
                    }
                }
            }

            Ok((other_rowers, boats_in_use))
        })
        .await
        .map_err(internal_error)?;

    let other_team_rower_display: Vec<templates::solve::OtherTeamRower> = other_team_rowers
        .into_iter()
        .map(|(rower, team_name)| templates::solve::OtherTeamRower { rower, team_name })
        .collect();

    // Apply walk-on overrides.
    for &id in &params.walkon {
        snapshot
            .availability
            .insert(id, lineup_db::availability::types::AvailabilityStatus::Yes);
    }

    // Apply no-shows — remove from availability so they don't appear in pool.
    for &id in &params.no_show {
        snapshot
            .availability
            .insert(id, lineup_db::availability::types::AvailabilityStatus::No);
    }

    // Parse seat placements from typed SeatTriples.
    let mut placements: std::collections::HashMap<
        BoatId,
        std::collections::HashMap<i32, lineup_db::rower::types::RowerId>,
    > = std::collections::HashMap::new();
    for t in &params.seat {
        placements
            .entry(t.boat_id)
            .or_default()
            .insert(t.seat.as_int(), t.rower_id);
    }

    // Parse active boats.
    let mut active_boats: std::collections::HashSet<BoatId> = params.boat.iter().copied().collect();

    // Handle boat-to-boat transfer: move rowers from source to dest
    // with seat-position mapping, then deactivate source.
    if let Some(ref transfer) = params.transfer {
        let src_id = transfer.source;
        let dst_id = transfer.dest;
        let src_boat = snapshot.boats.iter().find(|b| b.id == src_id);
        let dst_boat = snapshot.boats.iter().find(|b| b.id == dst_id);
        if let (Some(src), Some(dst)) = (src_boat, dst_boat) {
            let src_seats = placements.remove(&src_id).unwrap_or_default();
            let dst_seats = placements.remove(&dst_id).unwrap_or_default();
            let dst_is_populated = !dst_seats.is_empty();

            // Map src rowers -> dst boat.
            let new_dst = map_transfer_seats(&src_seats, src, dst);

            if dst_is_populated {
                // Live->live: also map dst rowers -> src boat (bidirectional swap).
                let new_src = map_transfer_seats(&dst_seats, dst, src);
                placements.insert(src_id, new_src);
                // Both boats stay active.
                active_boats.insert(dst_id);
            } else {
                // Source boat is deactivated.
                active_boats.remove(&src_id);
                active_boats.insert(dst_id);
            }

            placements.insert(dst_id, new_dst);
        }
    }

    // Parse locks for display.
    let locked_seats = SolveKnobs::triples_to_set(&params.lock);
    let pinned_seats = SolveKnobs::triples_to_set(&params.pin);
    let was_pinned_seats = SolveKnobs::triples_to_set(&params.was_pin);

    let editor =
        templates::solve::EditorData::from_placements(&snapshot, &placements, &active_boats);
    let flags = templates::solve::DisplayFlags {
        show_attributes: tenant.show_attributes(),
        force_cox_stern: tenant.config.force_cox_stern,
        locked_seats,
        pinned_seats,
        was_pinned_seats,
        pinned_boats: SolveKnobs::boat_id_set(&params.boat_pin),
        was_pinned_boats: SolveKnobs::boat_id_set(&params.boat_was_pin),
        locked_boats: SolveKnobs::boat_id_set(&params.boat_lock),
        boats_in_use_by: boats_in_use,
    };

    // Unavailable rowers for the walk-on dropdown.
    let walkon_ids = params.walkon;
    let unavailable: Vec<&lineup_db::rower::Rower> = snapshot
        .rowers
        .iter()
        .filter(|r| r.active.as_bool())
        .filter(|r| {
            !snapshot
                .availability
                .get(&r.id)
                .map(|s| s.is_available())
                .unwrap_or(snapshot.assume_available)
        })
        .collect();

    Ok(Html(
        templates::solve::lineup_editor(
            &snapshot,
            practice_id,
            &editor,
            &flags,
            &unavailable,
            &walkon_ids,
            &other_team_rower_display,
        )
        .into_string(),
    ))
}
