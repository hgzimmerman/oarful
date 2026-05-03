//! `POST /commit/{id}`, `POST /commit-lineup/{id}`, `POST /draft-lineup/{id}`,
//! and `POST /clear-draft/{id}` handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Extension,
};
use axum_extra::extract::CookieJar;
use lineup_db::app_user::Role;
use lineup_db::{
    lineup::{CommitSeat, Lineup},
    practice::{Practice, PracticeId},
    snapshot::DbSnapshot,
};
use lineup_solver::{ProposedLineup, SolveStatus};

use crate::extract::HtmlForm;
use crate::handlers::{bad_request, internal_error, ErrorResponse};
use crate::state::SolverCtx;

use super::*;

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn commit_handler(
    State(solver): State<SolverCtx>,
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(practice_id): Path<PracticeId>,
    HtmlForm(knobs): HtmlForm<SolveKnobs>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let team_id = _team_id;
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

    let date = practice.date;
    apply_no_shows(&mut snapshot, &knobs);
    let baselines = build_baselines(&knobs, &tenant.db, team_id).await?;
    let mut request = knobs.to_request(date, &snapshot, baselines);
    request.top_n = 1;

    let _permit = acquire_solve_permit(&solver).await?;
    let result = run_solve(&solver, snapshot.clone(), request).await?;

    if result.status != SolveStatus::Satisfied {
        tracing::warn!(?result.status, %practice_id, "refusing to commit non-satisfied solve");
        return Err(crate::handlers::ErrorResponse(
            StatusCode::CONFLICT,
            "Solver did not find a satisfying solution.".into(),
        ));
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
                        seat_position: lineup_db::lineup::SeatPosition::new(*seat),
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
        tenant.claims.audit_user_id(),
        "lineup.commit",
        "practice",
        &practice_id.to_string(),
        Some(serde_json::json!({"boat_count": boat_count}).to_string()),
    );
    tenant.complete_onboarding_step(lineup_db::onboarding::OnboardingStep::GenerateLineup);

    Ok(Redirect::to(&format!("/history/{practice_id}")))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn commit_lineup_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(practice_id): Path<PracticeId>,
    HtmlForm(input): HtmlForm<DirectCommitInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    // Parse "boat_id:seat_pos:rower_id" triples and group by boat.
    let mut by_boat: std::collections::BTreeMap<lineup_db::boat::types::BoatId, Vec<CommitSeat>> =
        std::collections::BTreeMap::new();
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
            seat_position: lineup_db::lineup::SeatPosition::new(seat_pos),
            rower_id,
            is_cox: seat_pos == 0,
        });
    }

    if by_boat.is_empty() {
        tracing::warn!(%practice_id, "direct commit with no valid seats");
        return Err(bad_request("No valid seats to commit."));
    }

    let boat_count = by_boat.len();
    let pid = practice_id;
    tenant
        .db
        .with_conn(move |conn| {
            // Verify the practice exists.
            let _practice = Practice::get(conn, pid)?.ok_or(diesel::result::Error::NotFound)?;
            for (boat_id, seats) in &by_boat {
                Lineup::commit_for_boat(conn, pid, *boat_id, seats)?;
            }
            Ok(())
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "lineup.commit",
        "practice",
        &practice_id.to_string(),
        Some(serde_json::json!({"boat_count": boat_count, "direct": true}).to_string()),
    );
    tenant.complete_onboarding_step(lineup_db::onboarding::OnboardingStep::GenerateLineup);

    Ok(Redirect::to(&format!("/history/{practice_id}")))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn draft_lineup_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(practice_id): Path<PracticeId>,
    HtmlForm(input): HtmlForm<DirectCommitInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let mut by_boat: std::collections::BTreeMap<lineup_db::boat::types::BoatId, Vec<CommitSeat>> =
        std::collections::BTreeMap::new();
    for entry in &input.seat {
        let parts: Vec<&str> = entry.splitn(3, ':').collect();
        if parts.len() != 3 {
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
            seat_position: lineup_db::lineup::SeatPosition::new(seat_pos),
            rower_id,
            is_cox: seat_pos == 0,
        });
    }

    if by_boat.is_empty() {
        return Err(bad_request("No valid seats to save as draft."));
    }

    let boats: Vec<(lineup_db::boat::types::BoatId, Vec<CommitSeat>)> =
        by_boat.into_iter().collect();
    let boat_count = boats.len();
    let pid = practice_id;
    tenant
        .db
        .with_conn(move |conn| {
            let _practice = Practice::get(conn, pid)?.ok_or(diesel::result::Error::NotFound)?;
            Lineup::save_draft_for_practice(conn, pid, &boats)?;
            Ok(())
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "lineup.draft",
        "practice",
        &practice_id.to_string(),
        Some(serde_json::json!({"boat_count": boat_count}).to_string()),
    );

    // Response body is unused — the success toast is triggered client-side
    // via hx-on::after-request on the form.
    Ok(StatusCode::NO_CONTENT)
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn clear_draft_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(practice_id): Path<PracticeId>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let pid = practice_id;
    tenant
        .db
        .with_conn(move |conn| {
            Lineup::delete_draft_for_practice(conn, pid)?;
            Ok(())
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "lineup.draft_clear",
        "practice",
        &practice_id.to_string(),
        None,
    );

    // Redirect to reload the solver page with a clean editor.
    Ok(Redirect::to(&format!("/solve/{practice_id}")))
}
