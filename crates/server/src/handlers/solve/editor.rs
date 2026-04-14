//! `GET /solve/{id}/editor` — re-render the lineup editor partial.

use axum::{
    extract::Path,
    http::StatusCode,
    response::Html,
    Extension,
};
use axum_extra::extract::{CookieJar, Query};
use lineup_db::boat::types::BoatId;
use lineup_db::snapshot::DbSnapshot;
use lineup_db::practice::{Practice, PracticeId};
use lineup_db::app_user::Role;

use crate::templates;
use crate::handlers::internal_error;

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
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let (practice, mut snapshot) = tenant
        .db
        .with_conn(move |conn| {
            let practice = Practice::get(conn, practice_id)?
                .ok_or(diesel::result::Error::NotFound)?;
            let snapshot = DbSnapshot::for_practice(conn, &practice)?;
            Ok((practice, snapshot))
        })
        .await
        .map_err(internal_error)?;

    let _date = practice.date;

    // Apply walk-on overrides.
    for id_str in &params.walkon {
        if let Ok(id) = id_str.parse::<lineup_db::rower::types::RowerId>() {
            snapshot.availability.insert(id, lineup_db::availability::types::AvailabilityStatus::Yes);
        }
    }

    // Apply no-shows — remove from availability so they don't appear in pool.
    for id_str in &params.no_show {
        if let Ok(id) = id_str.parse::<lineup_db::rower::types::RowerId>() {
            snapshot.availability.insert(id, lineup_db::availability::types::AvailabilityStatus::No);
        }
    }

    // Parse seat placements: "boat_id:seat_pos:rower_id"
    let mut placements: std::collections::HashMap<BoatId, std::collections::HashMap<i32, lineup_db::rower::types::RowerId>> =
        std::collections::HashMap::new();
    for entry in &params.seat {
        let parts: Vec<&str> = entry.splitn(3, ':').collect();
        if parts.len() != 3 { continue; }
        let Ok(boat_id) = parts[0].parse::<BoatId>() else { continue };
        let Ok(seat) = parts[1].parse::<i32>() else { continue };
        let Ok(rower_id) = parts[2].parse::<lineup_db::rower::types::RowerId>() else { continue };
        placements.entry(boat_id).or_default().insert(seat, rower_id);
    }

    // Parse active boats.
    let mut active_boats: std::collections::HashSet<BoatId> = params.boat.iter()
        .filter_map(|s| s.parse::<BoatId>().ok())
        .collect();

    // Handle boat-to-boat transfer: move rowers from source to dest
    // with seat-position mapping, then deactivate source.
    if let Some(ref transfer) = params.transfer {
        let parts: Vec<&str> = transfer.splitn(2, ':').collect();
        if parts.len() == 2 {
            if let (Ok(src_id), Ok(dst_id)) = (parts[0].parse::<BoatId>(), parts[1].parse::<BoatId>()) {
                let src_boat = snapshot.sweep_boats.iter().find(|b| b.id == src_id);
                let dst_boat = snapshot.sweep_boats.iter().find(|b| b.id == dst_id);
                if let (Some(src), Some(dst)) = (src_boat, dst_boat) {
                    let src_seats = placements.remove(&src_id).unwrap_or_default();
                    let dst_seats = placements.remove(&dst_id).unwrap_or_default();
                    let dst_is_populated = !dst_seats.is_empty();

                    // Map src rowers → dst boat.
                    let new_dst = map_transfer_seats(&src_seats, src, dst);

                    if dst_is_populated {
                        // Live→live: also map dst rowers → src boat (bidirectional swap).
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
        }
    }

    // Parse locks for display.
    let locked_seats: std::collections::HashSet<(lineup_db::rower::types::RowerId, BoatId, i32)> = params.lock.iter()
        .filter_map(|entry| {
            let parts: Vec<&str> = entry.splitn(3, ':').collect();
            if parts.len() != 3 { return None; }
            let rower_id = parts[0].parse().ok()?;
            let boat_id = parts[1].parse().ok()?;
            let seat = parts[2].parse().ok()?;
            Some((rower_id, boat_id, seat))
        })
        .collect();

    let pinned_seats = SolveKnobs::parse_triples(&params.pin);
    let was_pinned_seats = SolveKnobs::parse_triples(&params.was_pin);

    let editor = templates::solve::EditorData::from_placements(&snapshot, &placements, &active_boats);
    let flags = templates::solve::DisplayFlags {
        show_attributes: tenant.show_attributes(),
        force_cox_stern: tenant.config.force_cox_stern,
        locked_seats,
        pinned_seats,
        was_pinned_seats,
        pinned_boats: SolveKnobs::parse_boat_ids(&params.boat_pin),
        was_pinned_boats: SolveKnobs::parse_boat_ids(&params.boat_was_pin),
        locked_boats: SolveKnobs::parse_boat_ids(&params.boat_lock),
    };

    // Unavailable rowers for the walk-on dropdown.
    let walkon_ids = params.walkon;
    let unavailable: Vec<&lineup_db::rower::Rower> = snapshot.rowers.iter()
        .filter(|r| r.active.as_bool())
        .filter(|r| {
            !snapshot.availability
                .get(&r.id)
                .map(|s| s.is_available_for_sweep())
                .unwrap_or(false)
        })
        .collect();

    Ok(Html(
        templates::solve::lineup_editor(&snapshot, practice_id, &editor, &flags, &unavailable, &walkon_ids).into_string(),
    ))
}
