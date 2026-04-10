//! `GET /solve/{date}` — run the solver for a date and render the
//! proposed lineup, plus `POST /commit/{date}` to persist the primary.
//!
//! Solver parameters are exposed to the coach as a [`SolveKnobs`] form
//! on the solve view. The view extracts them from the URL query string
//! so a particular run is bookmarkable; the commit form re-sends them
//! as hidden fields so the persisted lineup matches the one the coach
//! actually saw.

use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    Form,
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
use serde::Deserialize;

use crate::{handlers::internal_error, state::AppState, templates};

/// Default UI solve budget, in seconds. Snappier than the CLI's 10s
/// because coaches expect an interactive response. See DESIGN.md
/// §"Solve invocation policy".
const DEFAULT_BUDGET_SECS: u64 = 3;

/// Default number of distinct lineups (primary + alternatives).
const DEFAULT_ALTS: usize = 3;

/// Coach-tunable solver knobs. Round-trips through both the URL query
/// string (for `GET /solve/{date}?...`) and the commit form body (as
/// hidden inputs), so the same struct can deserialise from both.
///
/// `serde` defaults handle the bare-URL case (`/solve/2026-04-11`)
/// without forcing every visitor to fill in a form.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SolveKnobs {
    /// Partial-fill budget. `0` means [`PartialFillPolicy::Strict`];
    /// any positive value means [`PartialFillPolicy::Allowed`] with
    /// that many empty optional seats permitted per boat.
    #[serde(default)]
    pub(crate) partial: i32,
    /// S7 novelty factor. Higher = more aggressive penalty against
    /// recently-used `(rower, boat, seat)` triples. `0` disables the
    /// term.
    #[serde(default)]
    pub(crate) novelty: i32,
    /// Number of distinct lineups to surface (primary + alternatives).
    /// Clamped to `1..` so `solve()` always runs.
    #[serde(default = "default_alts")]
    pub(crate) alts: usize,
    /// Time budget in seconds. Clamped to `1..` because Pumpkin treats
    /// a zero budget as "no propagation at all".
    #[serde(default = "default_budget")]
    pub(crate) budget: u64,
}

impl Default for SolveKnobs {
    fn default() -> Self {
        Self {
            partial: 0,
            novelty: 0,
            alts: DEFAULT_ALTS,
            budget: DEFAULT_BUDGET_SECS,
        }
    }
}

impl SolveKnobs {
    /// Build a [`SolveRequest`] for the given date. The fleet is left
    /// empty so the solver considers every in-service sweep boat.
    fn to_request(&self, date: NaiveDate) -> SolveRequest {
        SolveRequest {
            date,
            boats: vec![],
            partial_fill: if self.partial > 0 {
                PartialFillPolicy::Allowed(self.partial)
            } else {
                PartialFillPolicy::Strict
            },
            novelty_factor: self.novelty.max(0),
            config: SolverConfig::default(),
            time_budget: Some(Duration::from_secs(self.budget.max(1))),
            top_n: self.alts.max(1),
            tabu_min_diff: 2,
        }
    }
}

fn default_alts() -> usize {
    DEFAULT_ALTS
}

fn default_budget() -> u64 {
    DEFAULT_BUDGET_SECS
}

pub(crate) async fn view_handler(
    State(state): State<AppState>,
    Path(date): Path<NaiveDate>,
    Query(knobs): Query<SolveKnobs>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let snapshot = state
        .db
        .with_conn(move |conn| DbSnapshot::for_date(conn, date))
        .await
        .map_err(internal_error)?;

    // solve() is sync and may burn most of the time budget — keep it
    // off the async runtime via spawn_blocking.
    let request = knobs.to_request(date);
    let snapshot_for_solve = snapshot.clone();
    let result: SolveResult = tokio::task::spawn_blocking(move || {
        solve(&snapshot_for_solve, &request)
    })
    .await
    .map_err(internal_error)?
    .map_err(internal_error)?;

    let content = templates::solve::view_content(&snapshot, date, &knobs, &result);
    Ok(super::maybe_page(
        &format!("Solve · {date}"),
        content,
        hx,
    ))
}

pub(crate) async fn commit_handler(
    State(state): State<AppState>,
    Path(date): Path<NaiveDate>,
    Form(knobs): Form<SolveKnobs>,
) -> Result<impl IntoResponse, StatusCode> {
    // Re-solve with the same knobs the coach saw on the view page.
    // The solver is deterministic on a fixed snapshot + request, so
    // this reproduces the proposed lineup as long as the snapshot
    // hasn't changed between view and commit. Top-N is forced to 1
    // here because we only persist the primary regardless of how
    // many alternatives the view rendered.
    let snapshot = state
        .db
        .with_conn(move |conn| DbSnapshot::for_date(conn, date))
        .await
        .map_err(internal_error)?;

    let mut request = knobs.to_request(date);
    request.top_n = 1;

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
