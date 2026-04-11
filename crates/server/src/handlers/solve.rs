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
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use axum::Extension;
use axum_extra::extract::{CookieJar, Query};

use crate::extract::HtmlForm;
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
use tokio::sync::OwnedSemaphorePermit;

use crate::{handlers::internal_error, state::AppState, templates};
use lineup_db::app_user::Role;

/// Default per-alternative solve budget, in seconds. Total wall time
/// for one request is roughly `DEFAULT_BUDGET_SECS × DEFAULT_ALTS`
/// because each alternative is a separate tabu re-solve with its own
/// budget — see the lineup_cli `bench` axis C for the data.
///
/// **Why 1s**, measured on 10/14/20-rower fixtures with the small
/// 3-boat fleet:
///
/// | budget | wall (top_n=3) | outcome |
/// |--------|----------------|---------|
/// | 1s     | ~3s            | 5/5 satisfied at every roster size |
/// | 2s     | ~6s            | same |
/// | 3s     | ~9s            | same |
/// | 5s     | ~15s           | same |
///
/// The solver finds *a* feasible solution in milliseconds and spends
/// the rest of the budget searching for proven optimality. The 18/18
/// rower-placement count in `bench` axis A is identical at every
/// budget, so the marginal quality gained by going above 1s is at
/// best invisible to the coach. Drop to 1s; let the user crank it up
/// via the knob form when they care.
const DEFAULT_BUDGET_SECS: u64 = 1;

/// Default number of distinct lineups (primary + alternatives).
/// Alternatives are a marquee UI feature, so we keep the default at
/// 3 even though it triples the per-request wall time — the
/// `DEFAULT_BUDGET_SECS` decision above already accounts for the
/// multiplier.
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
    /// Novelty weight. Higher = more aggressive penalty against
    /// recently-used `(rower, boat, seat)` triples. `0` disables.
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
    /// Dates of committed practices to use as baselines. Each checked
    /// practice becomes a reference lineup with negative weight so the
    /// solver prefers reproducing it. Deserialized from repeated
    /// `based_on=YYYY-MM-DD` query params.
    #[serde(default)]
    pub(crate) based_on: Vec<String>,
    /// Similarity weight for baseline references. Higher = stronger
    /// preference for matching the baseline lineups. `0` disables
    /// even if `based_on` is non-empty.
    #[serde(default)]
    pub(crate) similarity: i32,
    /// Rower IDs to mark as no-show. Their availability is overridden
    /// to `No` before solving, so they're excluded from the lineup.
    /// Combined with `based_on` + `similarity` for no-show re-solve.
    #[serde(default)]
    pub(crate) no_show: Vec<String>,
    /// When present and non-zero, triggers the solver. Without this,
    /// the solve page shows knobs + existing lineups but doesn't run
    /// the solver — the coach clicks "Generate" to trigger it.
    #[serde(default)]
    pub(crate) generate: i32,
    /// Seat locks. Each value is `rower_id:boat_id:seat_position`.
    /// Repeated query param: `lock=1:2:8&lock=3:2:0`.
    #[serde(default)]
    pub(crate) lock: Vec<String>,
    /// Solver preset name. One of: balanced, even_speed, tiered, random.
    /// Overrides the default SolverConfig when present.
    #[serde(default)]
    pub(crate) preset: String,
}

impl Default for SolveKnobs {
    fn default() -> Self {
        Self {
            partial: 0,
            novelty: 0,
            alts: DEFAULT_ALTS,
            budget: DEFAULT_BUDGET_SECS,
            based_on: vec![],
            similarity: 0,
            no_show: vec![],
            generate: 0,
            lock: vec![],
            preset: String::new(),
        }
    }
}

impl SolveKnobs {
    /// Build a [`SolveRequest`] for the given date. Novelty
    /// reference lineups are built from the snapshot's recent
    /// placements — each historical practice becomes one
    /// [`ReferenceLineup`] with positive weight so the solver
    /// avoids repeating it. Cox seats are excluded (S6 handles
    /// cox rotation separately).
    fn to_request(
        &self,
        date: NaiveDate,
        snapshot: &DbSnapshot,
        baselines: Vec<lineup_solver::ReferenceLineup>,
    ) -> SolveRequest {
        use std::collections::BTreeMap;
        use lineup_solver::{ReferenceLineup, ReferencePlacement};

        let novelty_weight = self.novelty.max(0);
        let mut reference_lineups = Vec::new();

        if novelty_weight > 0 {
            let mut groups: BTreeMap<
                (NaiveDate, lineup_db::boat::types::BoatId),
                Vec<ReferencePlacement>,
            > = BTreeMap::new();
            for p in &snapshot.recent_placements {
                if p.is_cox || p.seat_position == 0 {
                    continue;
                }
                groups
                    .entry((p.practice_date, p.boat_id))
                    .or_default()
                    .push(ReferencePlacement {
                        rower_id: p.rower_id,
                        boat_id: p.boat_id,
                        seat: p.seat_position,
                    });
            }
            for placements in groups.into_values() {
                reference_lineups.push(ReferenceLineup {
                    placements,
                    weight: novelty_weight,
                });
            }
        }

        // Append caller-provided baselines (negative weight =
        // prefer similarity).
        reference_lineups.extend(baselines);

        SolveRequest {
            date,
            boats: vec![],
            partial_fill: if self.partial > 0 {
                PartialFillPolicy::Allowed(self.partial)
            } else {
                PartialFillPolicy::Strict
            },
            config: self.resolve_config(),
            time_budget: Some(Duration::from_secs(self.budget.max(1))),
            top_n: self.alts.max(1),
            tabu_min_diff: 2,
            reference_lineups,
            locks: self.parse_locks(),
        }
    }

    /// Resolve the SolverConfig from the preset name. Built-in presets
    /// are checked first; custom profiles are resolved later in the
    /// handler (requires DB access).
    fn resolve_config(&self) -> SolverConfig {
        SolverConfig::from_preset(&self.preset).unwrap_or_default()
    }

    /// Parse `lock` query params into `SeatLock`s.
    /// Each value is `rower_id:boat_id:seat_position`.
    fn parse_locks(&self) -> Vec<lineup_solver::SeatLock> {
        self.lock
            .iter()
            .filter_map(|entry| {
                let parts: Vec<&str> = entry.splitn(3, ':').collect();
                if parts.len() != 3 {
                    return None;
                }
                let rower_id = parts[0].parse().ok()?;
                let boat_id = parts[1].parse().ok()?;
                let seat = parts[2].parse().ok()?;
                Some(lineup_solver::SeatLock {
                    rower_id,
                    boat_id,
                    seat,
                })
            })
            .collect()
    }
}

/// Build baseline reference lineups from the `based_on` dates in the
/// knobs. Each checked practice's committed lineup becomes one
/// `ReferenceLineup` with negative weight (prefer similarity).
async fn build_baselines(
    knobs: &SolveKnobs,
    db: &lineup_db::state::Db,
    team_id: lineup_db::team::TeamId,
) -> Result<Vec<lineup_solver::ReferenceLineup>, StatusCode> {
    use lineup_solver::{ReferenceLineup, ReferencePlacement};

    let similarity = knobs.similarity.max(0);
    if similarity == 0 || knobs.based_on.is_empty() {
        return Ok(vec![]);
    }

    let dates: Vec<NaiveDate> = knobs
        .based_on
        .iter()
        .filter_map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .collect();
    if dates.is_empty() {
        return Ok(vec![]);
    }

    let refs = db
        .with_conn(move |conn| {
            let mut refs = Vec::new();
            for date in &dates {
                let Some(practice) = Practice::find_by_date(conn, team_id, *date)? else {
                    continue;
                };
                let committed = Lineup::for_practice(conn, practice.id)?;
                let placements: Vec<ReferencePlacement> = committed
                    .iter()
                    .flat_map(|c| {
                        c.seats.iter().map(|s| ReferencePlacement {
                            rower_id: s.rower_id,
                            boat_id: c.lineup.boat_id,
                            seat: s.seat_position,
                        })
                    })
                    .collect();
                if !placements.is_empty() {
                    refs.push(ReferenceLineup {
                        placements,
                        weight: -similarity,
                    });
                }
            }
            Ok(refs)
        })
        .await
        .map_err(internal_error)?;

    Ok(refs)
}

/// Override availability to `No` for any rower IDs listed in
/// `knobs.no_show`. Mutates the snapshot in place.
fn profile_to_config(p: &lineup_db::solver_profile::SolverProfile) -> SolverConfig {
    SolverConfig {
        skill_variance_weight: p.skill_variance_weight,
        pair_affinity_weight: p.pair_affinity_weight,
        seat_affinity_weight: p.seat_affinity_weight,
        side_preference_weight: p.side_preference_weight,
        weight_class_slack_weight: p.weight_class_slack_weight,
        cox_cooldown_penalty: p.cox_cooldown_penalty,
        placement_reward_weight: p.placement_reward_weight,
        pair_strength_weight: p.pair_strength_weight,
        bow_pair_strength_weight: p.bow_pair_strength_weight,
        height_balance_weight: p.height_balance_weight,
        end_pair_skill_weight: p.end_pair_skill_weight,
        engine_room_strength_weight: p.engine_room_strength_weight,
        partial_fill_bonus: p.partial_fill_bonus,
        non_scull_retention_weight: p.non_scull_retention_weight,
        bow_cox_fit_weight: p.bow_cox_fit_weight,
    }
}

fn apply_no_shows(snapshot: &mut DbSnapshot, knobs: &SolveKnobs) {
    use lineup_db::availability::types::AvailabilityStatus;
    use lineup_db::rower::types::RowerId;

    for id_str in &knobs.no_show {
        if let Ok(id) = id_str.parse::<RowerId>() {
            snapshot.availability.insert(id, AvailabilityStatus::No);
        }
    }
}

fn default_alts() -> usize {
    DEFAULT_ALTS
}

fn default_budget() -> u64 {
    DEFAULT_BUDGET_SECS
}

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
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
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

    // Only run the solver when explicitly requested via generate=1.
    if knobs.generate > 0 {
        apply_no_shows(&mut snapshot, &knobs);
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
        let result = run_solve(&state, snapshot.clone(), request).await?;

        let locked_seats = knobs.parse_locks().into_iter()
            .map(|l| (l.rower_id, l.boat_id, l.seat))
            .collect();
        let flags = templates::solve::DisplayFlags {
            show_attributes: tenant.show_attributes(),
            force_cox_stern: tenant.config.force_cox_stern,
            locked_seats,
        };
        let profile_names: Vec<String> = custom_profiles.iter().map(|p| p.name.clone()).collect();
        let content = templates::solve::view_content(
            &snapshot, date, &knobs, &result, &committed_practices,
            &flags, &profile_names,
        );
        return Ok(super::maybe_page(
            &format!("Generate · {date}"),
            content,
            hx,
        ));
    }

    // Landing page: show knobs + "Generate" / "Re-generate" button.
    let profile_names: Vec<String> = custom_profiles.iter().map(|p| p.name.clone()).collect();
    let content = templates::solve::landing_content(
        &snapshot, date, &knobs, &committed_practices, has_committed,
        &profile_names,
    );
    Ok(super::maybe_page(
        &format!("Generate · {date}"),
        content,
        hx,
    ))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn commit_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(date): Path<NaiveDate>,
    HtmlForm(knobs): HtmlForm<SolveKnobs>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let mut snapshot = tenant
        .db
        .with_conn(move |conn| DbSnapshot::for_team_date(conn, team_id, date))
        .await
        .map_err(internal_error)?;

    apply_no_shows(&mut snapshot, &knobs);
    let baselines = build_baselines(&knobs, &tenant.db, team_id).await?;
    let mut request = knobs.to_request(date, &snapshot, baselines);
    request.top_n = 1;

    let _permit = acquire_solve_permit(&state).await?;
    let result = run_solve(&state, snapshot.clone(), request).await?;

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

    tenant
        .db
        .with_conn(move |conn| {
            let practice = Practice::upsert_by_date(conn, team_id, date, None)?;
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

// =====================================================================
// Direct commit (no re-solve) — used by the manual-swap UI
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct DirectCommitInput {
    /// Repeated field: each value is "boat_id:seat_pos:rower_id".
    #[serde(default)]
    seat: Vec<String>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn commit_lineup_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(date): Path<NaiveDate>,
    HtmlForm(input): HtmlForm<DirectCommitInput>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

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
        tracing::warn!(%date, "direct commit with no valid seats");
        return Err(StatusCode::BAD_REQUEST);
    }

    tenant
        .db
        .with_conn(move |conn| {
            let practice = Practice::upsert_by_date(conn, team_id, date, None)?;
            for (boat_id, seats) in &by_boat {
                Lineup::commit_for_boat(conn, practice.id, *boat_id, seats)?;
            }
            Ok(())
        })
        .await
        .map_err(internal_error)?;

    Ok(Redirect::to(&format!("/history/{date}")))
}

/// Dispatch `solve()` onto the dedicated rayon thread pool via a
/// oneshot channel. The async handler awaits the result without
/// touching tokio's blocking pool — DB queries via deadpool-diesel
/// are unaffected regardless of how many concurrent solves are
/// in flight.
// =====================================================================
// Save solver profile
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct SaveProfileInput {
    name: String,
    preset: String,
}

/// `POST /solver-profile` — save the current preset as a custom profile.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn save_profile_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    axum::Form(input): axum::Form<SaveProfileInput>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Resolve the config from the preset (built-in or default).
    let config = SolverConfig::from_preset(&input.preset).unwrap_or_default();

    let new_profile = lineup_db::solver_profile::NewSolverProfile {
        team_id,
        name,
        skill_variance_weight: config.skill_variance_weight,
        pair_affinity_weight: config.pair_affinity_weight,
        seat_affinity_weight: config.seat_affinity_weight,
        side_preference_weight: config.side_preference_weight,
        weight_class_slack_weight: config.weight_class_slack_weight,
        cox_cooldown_penalty: config.cox_cooldown_penalty,
        placement_reward_weight: config.placement_reward_weight,
        pair_strength_weight: config.pair_strength_weight,
        bow_pair_strength_weight: config.bow_pair_strength_weight,
        height_balance_weight: config.height_balance_weight,
        end_pair_skill_weight: config.end_pair_skill_weight,
        engine_room_strength_weight: config.engine_room_strength_weight,
        partial_fill_bonus: config.partial_fill_bonus,
        non_scull_retention_weight: config.non_scull_retention_weight,
        bow_cox_fit_weight: config.bow_cox_fit_weight,
    };

    tenant
        .db
        .with_conn(move |conn| {
            lineup_db::solver_profile::SolverProfile::upsert(conn, new_profile)
        })
        .await
        .map_err(internal_error)?;

    // Redirect back to the referring page (or practices).
    Ok(Redirect::to("/practices"))
}

async fn run_solve(
    state: &AppState,
    snapshot: DbSnapshot,
    request: SolveRequest,
) -> Result<SolveResult, StatusCode> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.solver_pool.spawn(move || {
        let result = solve(&snapshot, &request);
        let _ = tx.send(result);
    });
    rx.await
        .map_err(internal_error)? // oneshot cancelled (rayon panicked)
        .map_err(internal_error) // solve() returned Err
}

/// Acquire one slot on the global solve semaphore. Returns immediately
/// when capacity is available; otherwise blocks the request future
/// until another solve completes. Logs queue depth so operators can
/// see the cap biting in production.
///
/// The permit is owned (`OwnedSemaphorePermit`) so callers can hold
/// it across an `await` for `spawn_blocking` without lifetime
/// gymnastics. Drop the permit by letting it fall out of scope.
async fn acquire_solve_permit(
    state: &AppState,
) -> Result<OwnedSemaphorePermit, StatusCode> {
    if let Ok(permit) = state.solve_semaphore.clone().try_acquire_owned() {
        return Ok(permit);
    }
    let queue_start = std::time::Instant::now();
    tracing::info!(
        capacity = state.solve_semaphore.available_permits(),
        "solve queued — semaphore at capacity"
    );
    let permit = state
        .solve_semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(internal_error)?;
    tracing::info!(
        queued_for = ?queue_start.elapsed(),
        "solve dequeued"
    );
    Ok(permit)
}
