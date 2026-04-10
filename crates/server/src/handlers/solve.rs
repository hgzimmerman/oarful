//! `GET /solve/{date}` — run the solver for a date and render the
//! proposed lineup, plus `POST /commit/{date}` to persist the primary.

use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use axum_htmx::HxRequest;
use chrono::NaiveDate;
use lineup_db::{
    lineup::{CommitSeat, Lineup},
    practice::Practice,
    snapshot::DbSnapshot,
};
use lineup_solver::{
    solve, PartialFillPolicy, ProposedLineup, SolveRequest, SolveResult, SolveStatus,
    SolverConfig,
};

use crate::{handlers::internal_error, state::AppState, templates};

/// Default UI solve budget. Snappier than the CLI's 10s because coaches
/// expect an interactive response. See DESIGN.md §"Solve invocation
/// policy".
const DEFAULT_TIME_BUDGET: Duration = Duration::from_secs(3);

/// Default number of alternatives to surface alongside the primary.
const DEFAULT_TOP_N: usize = 3;

pub async fn view_handler(
    State(state): State<AppState>,
    Path(date): Path<NaiveDate>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let snapshot = state
        .db
        .with_conn(move |conn| DbSnapshot::for_date(conn, date))
        .await
        .map_err(internal_error)?;

    // Run solve() on the blocking pool — solve() is sync and can burn
    // up to the time budget, so we don't want to tie up a tokio worker.
    let request = SolveRequest {
        date,
        boats: vec![],
        partial_fill: PartialFillPolicy::Strict,
        novelty_factor: 0,
        config: SolverConfig::default(),
        time_budget: Some(DEFAULT_TIME_BUDGET),
        top_n: DEFAULT_TOP_N,
        tabu_min_diff: 2,
    };
    let snapshot_for_solve = snapshot.clone();
    let result: SolveResult = tokio::task::spawn_blocking(move || {
        solve(&snapshot_for_solve, &request)
    })
    .await
    .map_err(internal_error)?
    .map_err(internal_error)?;

    let content = templates::solve::view_content(&snapshot, date, &result);
    Ok(super::maybe_page(
        &format!("Solve · {date}"),
        content,
        hx,
    ))
}

pub async fn commit_handler(
    State(state): State<AppState>,
    Path(date): Path<NaiveDate>,
) -> Result<impl IntoResponse, StatusCode> {
    // Re-run the solver so the commit operates on a fresh result. This
    // is wasteful under the no-edit MVP (the user already saw the
    // proposed lineup on the previous page) but it keeps the POST
    // stateless — no server-side session carrying `ProposedLineup`s.
    // When we add the knob form we'll thread the user's choices
    // through as form fields here.
    let snapshot = state
        .db
        .with_conn(move |conn| DbSnapshot::for_date(conn, date))
        .await
        .map_err(internal_error)?;

    let request = SolveRequest {
        date,
        boats: vec![],
        partial_fill: PartialFillPolicy::Strict,
        novelty_factor: 0,
        config: SolverConfig::default(),
        time_budget: Some(DEFAULT_TIME_BUDGET),
        top_n: 1,
        tabu_min_diff: 2,
    };
    let snapshot_for_solve = snapshot.clone();
    let result = tokio::task::spawn_blocking(move || {
        solve(&snapshot_for_solve, &request)
    })
    .await
    .map_err(internal_error)?
    .map_err(internal_error)?;

    if result.status != SolveStatus::Satisfied {
        tracing::warn!(?result.status, %date, "refusing to commit non-satisfied solve");
        return Err(StatusCode::CONFLICT);
    }

    let used: Vec<ProposedLineup> = result
        .primary
        .lineups
        .into_iter()
        .filter(|l| l.used)
        .collect();

    state
        .db
        .with_conn(move |conn| {
            let practice = Practice::upsert_by_date(conn, date, None)?;
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
                Lineup::commit_for_boat(conn, practice.id, lineup.boat_id, &seats)?;
            }
            Ok(())
        })
        .await
        .map_err(internal_error)?;

    Ok(Redirect::to(&format!("/history/{date}")))
}
