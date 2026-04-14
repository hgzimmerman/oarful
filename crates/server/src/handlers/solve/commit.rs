//! `POST /commit/{id}` and `POST /commit-lineup/{id}` handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Extension,
};
use axum_extra::extract::CookieJar;
use lineup_db::{
    lineup::{CommitSeat, Lineup},
    practice::{Practice, PracticeId},
    snapshot::DbSnapshot,
};
use lineup_solver::{ProposedLineup, SolveStatus};
use lineup_db::app_user::Role;

use crate::extract::HtmlForm;
use crate::handlers::internal_error;
use crate::state::AppState;

use super::*;

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn commit_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(practice_id): Path<PracticeId>,
    HtmlForm(knobs): HtmlForm<SolveKnobs>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let team_id = _team_id;
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

    let date = practice.date;
    apply_no_shows(&mut snapshot, &knobs);
    let baselines = build_baselines(&knobs, &tenant.db, team_id).await?;
    let mut request = knobs.to_request(date, &snapshot, baselines);
    request.top_n = 1;

    let _permit = acquire_solve_permit(&state).await?;
    let result = run_solve(&state, snapshot.clone(), request).await?;

    if result.status != SolveStatus::Satisfied {
        tracing::warn!(?result.status, %practice_id, "refusing to commit non-satisfied solve");
        return Err(StatusCode::CONFLICT);
    }

    let used: Vec<ProposedLineup> = result
        .primary
        .lineups
        .into_iter()
        .filter(|l| l.used)
        .collect();

    let boat_count = used.len();
    let pid = practice.id;
    tenant
        .db
        .with_conn(move |conn| {
            for lineup in &used {
                let seats: Vec<CommitSeat> = lineup
                    .seats
                    .iter()
                    .map(|(seat, rower_id)| CommitSeat {
                        seat_position: *seat,
                        rower_id: *rower_id,
                        is_cox: *seat == 0,
                    })
                    .collect();
                Lineup::commit_for_boat(conn, pid, lineup.boat_id, &seats)?;
            }
            Ok(())
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "lineup.commit",
        "practice",
        &practice_id.to_string(),
        Some(serde_json::json!({"boat_count": boat_count}).to_string()),
    );

    Ok(Redirect::to(&format!("/history/{practice_id}")))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn commit_lineup_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(practice_id): Path<PracticeId>,
    HtmlForm(input): HtmlForm<DirectCommitInput>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    // Parse "boat_id:seat_pos:rower_id" triples and group by boat.
    let mut by_boat: std::collections::BTreeMap<
        lineup_db::boat::types::BoatId,
        Vec<CommitSeat>,
    > = std::collections::BTreeMap::new();
    for entry in &input.seat {
        let parts: Vec<&str> = entry.splitn(3, ':').collect();
        if parts.len() != 3 {
            tracing::warn!(entry, "malformed seat field, skipping");
            continue;
        }
        let Ok(boat_id) = parts[0].parse::<lineup_db::boat::types::BoatId>() else {
            continue;
        };
        let Ok(seat_pos) = parts[1].parse::<i32>() else {
            continue;
        };
        let Ok(rower_id) = parts[2].parse::<lineup_db::rower::types::RowerId>() else {
            continue;
        };
        by_boat.entry(boat_id).or_default().push(CommitSeat {
            seat_position: seat_pos,
            rower_id,
            is_cox: seat_pos == 0,
        });
    }

    if by_boat.is_empty() {
        tracing::warn!(%practice_id, "direct commit with no valid seats");
        return Err(StatusCode::BAD_REQUEST);
    }

    let boat_count = by_boat.len();
    let pid = practice_id;
    tenant
        .db
        .with_conn(move |conn| {
            // Verify the practice exists.
            let _practice = Practice::get(conn, pid)?
                .ok_or(diesel::result::Error::NotFound)?;
            for (boat_id, seats) in &by_boat {
                Lineup::commit_for_boat(conn, pid, *boat_id, seats)?;
            }
            Ok(())
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "lineup.commit",
        "practice",
        &practice_id.to_string(),
        Some(serde_json::json!({"boat_count": boat_count, "direct": true}).to_string()),
    );

    Ok(Redirect::to(&format!("/history/{practice_id}")))
}
