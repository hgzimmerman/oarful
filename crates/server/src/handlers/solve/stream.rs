//! `GET /solve/{id}/stream` — SSE endpoint that streams solver results.

use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Extension,
};
use axum_extra::extract::{CookieJar, Query};
use futures::stream::Stream;
use lineup_db::app_user::Role;
use lineup_db::practice::PracticeId;
use lineup_db::snapshot::DbSnapshot;
use lineup_solver::SolveStreamEvent;
use tokio::sync::mpsc;

use crate::handlers::{internal_error, ErrorResponse};
use crate::state::SolverCtx;
use crate::templates;

use super::*;

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn stream_handler(
    State(solver): State<SolverCtx>,
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(practice_id): Path<PracticeId>,
    Query(knobs): Query<SolveKnobs>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let (practice, mut snapshot) = tenant
        .db
        .with_conn(move |conn| {
            let practice = lineup_db::practice::Practice::get(conn, practice_id)?
                .ok_or(diesel::result::Error::NotFound)?;
            let snapshot = DbSnapshot::for_practice(conn, &practice)?;
            Ok((practice, snapshot))
        })
        .await
        .map_err(internal_error)?;

    let date = practice.date;
    apply_walkons(&mut snapshot, &knobs);
    apply_no_shows(&mut snapshot, &knobs);

    let baselines = build_baselines(&knobs, &tenant.db, team_id).await?;
    let permit = acquire_solve_permit(&solver).await?;

    let custom_profiles = tenant
        .db
        .with_conn(move |conn| {
            lineup_db::solver_profile::SolverProfile::list_for_team(conn, team_id)
        })
        .await
        .map_err(internal_error)?;

    let preset_name = knobs.preset.clone();
    let config = custom_profiles
        .iter()
        .find(|p| p.name == preset_name)
        .map(profile_to_config)
        .unwrap_or_else(|| knobs.resolve_config());

    let mut request = knobs.to_request(date, &snapshot, baselines);
    request.config = config;
    let mut solver_locks = knobs.parse_locks();
    for t in &knobs.pin {
        solver_locks.push(lineup_solver::SeatLock {
            rower_id: t.rower_id,
            boat_id: t.boat_id,
            seat: t.seat.as_int(),
        });
    }
    request.locks = solver_locks;

    // Display flags for rendering SSE event payloads.
    let pinned_rowers: std::collections::HashSet<lineup_db::rower::types::RowerId> =
        knobs.pin.iter().map(|t| t.rower_id).collect();

    let flags = templates::solve::DisplayFlags {
        show_attributes: tenant.show_attributes(),
        force_cox_stern: tenant.config.force_cox_stern,
        locked_seats: SolveKnobs::triples_to_set(&knobs.lock),
        pinned_seats: std::collections::HashSet::new(),
        was_pinned_seats: std::collections::HashSet::new(),
        pinned_boats: std::collections::HashSet::new(),
        was_pinned_boats: SolveKnobs::boat_id_set(&knobs.boat_was_pin),
        locked_boats: SolveKnobs::boat_id_set(&knobs.boat_lock),
        boats_in_use_by: std::collections::HashMap::new(),
    };

    // Spawn solver on rayon, bridging to a tokio channel.
    let (tokio_tx, mut tokio_rx) = mpsc::channel::<SolveStreamEvent>(4);
    let snapshot_for_solve = snapshot.clone();
    solver.solver_pool.spawn(move || {
        // Bridge: std::sync channel for the solver, forwarded to tokio channel.
        let (std_tx, std_rx) = std::sync::mpsc::sync_channel::<SolveStreamEvent>(2);
        let fwd_handle = std::thread::spawn({
            let tokio_tx = tokio_tx;
            move || {
                while let Ok(event) = std_rx.recv() {
                    if tokio_tx.blocking_send(event).is_err() {
                        break; // receiver dropped
                    }
                }
            }
        });
        if let Err(e) =
            lineup_solver::solve_streaming(&snapshot_for_solve, &request, std_tx.clone())
        {
            tracing::error!(?e, "streaming solver error");
            // Send a failure event so the SSE stream can close cleanly.
            let _ = std_tx.send(SolveStreamEvent::PrimaryFailed {
                status: lineup_solver::SolveStatus::Unsatisfiable,
                diagnostics: vec![],
            });
            let _ = std_tx.send(SolveStreamEvent::Done {
                elapsed: std::time::Duration::ZERO,
            });
        }
        let _ = fwd_handle.join();
    });

    let unavailable: Vec<lineup_db::rower::types::RowerId> = snapshot
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
        .map(|r| r.id)
        .collect();

    let knobs_clone = knobs.clone();
    let stream = async_stream::stream! {
        let _permit = permit; // hold semaphore until stream ends

        while let Some(event) = tokio_rx.recv().await {
            match event {
                SolveStreamEvent::Primary { solution, .. } => {
                    // Compute was_pin transitions for the response knobs.
                    let was_pinned_seats: std::collections::HashSet<_> = solution.lineups.iter()
                        .filter(|l| l.used)
                        .flat_map(|l| l.seats.iter().map(move |&(seat, rid)| (rid, l.boat_id, seat)))
                        .filter(|(rid, _, _)| pinned_rowers.contains(rid))
                        .collect();
                    let mut response_knobs = knobs_clone.clone();
                    response_knobs.was_pin = was_pinned_seats.iter()
                        .map(|(rid, bid, seat)| SeatTriple {
                            rower_id: *rid,
                            boat_id: *bid,
                            seat: lineup_db::lineup::SeatPosition::new(*seat),
                        })
                        .collect();
                    response_knobs.pin = vec![];
                    response_knobs.boat_was_pin = knobs_clone.boat_pin.clone();
                    response_knobs.boat_pin = vec![];

                    let mut render_flags = flags.clone();
                    render_flags.was_pinned_seats = was_pinned_seats;

                    let editor = templates::solve::EditorData::from_solve(&snapshot, &solution);
                    let unavail_rowers: Vec<_> = snapshot.rowers.iter()
                        .filter(|r| unavailable.contains(&r.id))
                        .collect();

                    let editor_html = templates::solve::lineup_editor(
                        &snapshot, practice_id, &editor, &render_flags, false,
                    );
                    let pool_oob = templates::solve::roster_pool_oob(
                        &snapshot, practice_id, &editor,
                        &unavail_rowers, &knobs_clone.walkon, &[],
                    );

                    // Build gatherState-format params for saving to the
                    // active tab cookie.
                    let mut tab_params = Vec::new();
                    for lineup in solution.lineups.iter().filter(|l| l.used) {
                        tab_params.push(format!("boat={}", lineup.boat_id));
                        for (seat, rower_id) in &lineup.seats {
                            tab_params.push(format!(
                                "seat={}:{}:{}",
                                rower_id, lineup.boat_id, seat
                            ));
                        }
                    }
                    let tab_state_str = tab_params.join("&");

                    // Wrap editor in a #solve-results div with OOB swap
                    // so it replaces the existing editor in-place without
                    // blanking the page during generation.
                    let editor_oob = maud::html! {
                        div #solve-results hx-swap-oob="innerHTML" {
                            (maud::PreEscaped(editor_html.into_string()))
                        }
                    };
                    // Include a script to save the primary result to the
                    // active tab cookie so tab switching preserves it.
                    let save_script = format!(
                        "<script>if(typeof setTabState==='function'){{var m=getEditorTabs();setTabState(m.active,{});var p=document.querySelector('#tab-bar [data-tab-id=\"'+m.active+'\"]');if(p){{p.classList.remove('tab-pending');p.removeAttribute('data-pending');}}}}</script>",
                        serde_json::json!(tab_state_str),
                    );
                    yield Ok(Event::default().event("primary").data(
                        format!("{}{}{}", editor_oob.into_string(), pool_oob.into_string(), save_script)
                    ));
                }
                SolveStreamEvent::PrimaryFailed { status, diagnostics } => {
                    let result = lineup_solver::SolveResult {
                        status,
                        primary: lineup_solver::ProposedSolution::default(),
                        alternatives: vec![],
                        diagnostics,
                        elapsed: std::time::Duration::ZERO,
                        objective: None,
                        cp_breakdown: vec![],
                    };
                    let status_msg = match result.status {
                        lineup_solver::SolveStatus::Unsatisfiable =>
                            "No valid lineup exists with these constraints. Try relaxing boat selection or partial fill.",
                        _ =>
                            "Ran out of time without finding a valid lineup. Try increasing the time budget or relaxing constraints.",
                    };
                    // Send as a "done" event so sse-close="done" cleanly
                    // closes the connection. The script stops the button
                    // animation and shows the error toast.
                    let script = format!(
                        "<script>stopGenerating(); showErrorToast({});</script>",
                        serde_json::json!(status_msg),
                    );
                    yield Ok(Event::default().event("done").data(script));
                    break;
                }
                SolveStreamEvent::Alternative { index, solution } => {
                    // Emit a "tab" event with a <script> that creates a
                    // new tab from the alternative's seat data.
                    let used: Vec<&lineup_solver::ProposedLineup> =
                        solution.lineups.iter().filter(|l| l.used).collect();
                    let mut params = Vec::new();
                    for lineup in &used {
                        params.push(format!("boat={}", lineup.boat_id));
                        for (seat, rower_id) in &lineup.seats {
                            params.push(format!(
                                "seat={}:{}:{}",
                                rower_id, lineup.boat_id, seat
                            ));
                        }
                    }
                    let label = format!("Alt {}", index + 1);
                    let seat_params = params.join("&");
                    let script = format!(
                        "<script>createTabFromSSE({}, {})</script>",
                        serde_json::json!(label),
                        serde_json::json!(seat_params),
                    );
                    yield Ok(Event::default().event("tab").data(script));
                }
                SolveStreamEvent::Done { elapsed } => {
                    let ms = elapsed.as_millis();
                    let label = if ms < 1000 {
                        format!("{ms} ms")
                    } else {
                        format!("{:.1}s", elapsed.as_secs_f64())
                    };
                    let script = format!(
                        "<script>stopGenerating({})</script>",
                        serde_json::json!(label),
                    );
                    yield Ok(Event::default().event("done").data(script));
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
