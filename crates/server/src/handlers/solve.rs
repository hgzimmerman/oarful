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
    boat::types::BoatId,
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
const DEFAULT_BUDGET_SECS: u64 = 3;

/// Default number of *additional* alternatives beyond the primary.
/// The UI shows this directly (0 = primary only, 1-3 = extra lineups).
/// The solver receives `top_n = alts + 1`. Default 0 — alternatives
/// burn time budget and the coach usually just wants one good lineup.
const DEFAULT_ALTS: usize = 0;

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
    /// Seat locks (explicit coach locks). Each value is
    /// `rower_id:boat_id:seat_position`.
    #[serde(default)]
    pub(crate) lock: Vec<String>,
    /// Dirty pins (manual edits since last generate). Same format.
    /// Honored as solver constraints on Generate, then converted
    /// to `was_pin` in the response.
    #[serde(default)]
    pub(crate) pin: Vec<String>,
    /// Was-pinned seats (honored last generate, no longer constrained).
    /// Coach can promote to lock via the icon.
    #[serde(default)]
    pub(crate) was_pin: Vec<String>,
    /// Solver preset name. One of: balanced, even_speed, tiered, random.
    /// Overrides the default SolverConfig when present.
    #[serde(default)]
    pub(crate) preset: String,
    /// Walk-on rower IDs (as strings — parsed to `RowerId` in
    /// `apply_walkons`). Rowers who showed up without having set
    /// availability. Their availability is transiently overridden to
    /// "Yes" for this solve session (no DB write). Deserialized from
    /// repeated `walkon=<rower_id>` query params.
    #[serde(default)]
    pub(crate) walkon: Vec<String>,
    /// Active boat IDs from the editor. When non-empty, restricts the
    /// solver to only consider these boats. Deserialized from repeated
    /// `boat=<boat_id>` query params injected by the editor JS.
    #[serde(default)]
    pub(crate) boat: Vec<String>,
    /// Dirty-pinned boats (must be fielded this generate).
    #[serde(default)]
    pub(crate) boat_pin: Vec<String>,
    /// Was-pinned boats (honored last generate, free next time).
    #[serde(default)]
    pub(crate) boat_was_pin: Vec<String>,
    /// Locked boats (always fielded).
    #[serde(default)]
    pub(crate) boat_lock: Vec<String>,
    /// Pre-populated seat placements for the editor. Each value is
    /// `boat_id:seat_pos:rower_id`. Used when navigating from the
    /// history page "Edit lineup" button.
    #[serde(default)]
    pub(crate) seat: Vec<String>,
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
            pin: vec![],
            was_pin: vec![],
            preset: String::new(),
            walkon: vec![],
            boat: vec![],
            boat_pin: vec![],
            boat_was_pin: vec![],
            boat_lock: vec![],
            seat: vec![],
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

        // Active boats + any boats referenced by seat locks/pins + boat pins/locks.
        let mut boat_set: std::collections::HashSet<BoatId> = self.boat.iter()
            .filter_map(|s| s.parse::<BoatId>().ok())
            .collect();
        for entries in [&self.lock, &self.pin] {
            for entry in entries.iter() {
                let parts: Vec<&str> = entry.splitn(3, ':').collect();
                if parts.len() == 3 {
                    if let Ok(bid) = parts[1].parse::<BoatId>() {
                        boat_set.insert(bid);
                    }
                }
            }
        }
        for entries in [&self.boat_pin, &self.boat_lock] {
            for entry in entries.iter() {
                if let Ok(bid) = entry.parse::<BoatId>() {
                    boat_set.insert(bid);
                }
            }
        }
        let boats: Vec<BoatId> = boat_set.into_iter().collect();
        // Required boats: dirty-pinned + locked boats must be fielded.
        let mut required_boats: Vec<BoatId> = self.boat_pin.iter()
            .chain(self.boat_lock.iter())
            .filter_map(|s| s.parse::<BoatId>().ok())
            .collect();
        required_boats.sort();
        required_boats.dedup();

        SolveRequest {
            date,
            boats,
            partial_fill: if self.partial > 0 {
                PartialFillPolicy::Allowed(self.partial)
            } else {
                PartialFillPolicy::Strict
            },
            config: self.resolve_config(),
            time_budget: Some(Duration::from_secs(self.budget.max(1))),
            top_n: (self.alts + 1).max(1),
            tabu_min_diff: 2,
            reference_lineups,
            locks: self.parse_locks(),
            required_boats,
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
    fn parse_boat_ids(entries: &[String]) -> std::collections::HashSet<BoatId> {
        entries.iter().filter_map(|s| s.parse().ok()).collect()
    }

    fn parse_triples(entries: &[String]) -> std::collections::HashSet<(lineup_db::rower::types::RowerId, BoatId, i32)> {
        entries.iter().filter_map(|entry| {
            let parts: Vec<&str> = entry.splitn(3, ':').collect();
            if parts.len() != 3 { return None; }
            Some((parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?))
        }).collect()
    }

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
/// `GET /solve/{date}/preset-bar` — HTMX partial returning just the
/// preset selector bar with updated active state.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn preset_bar_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(date): Path<NaiveDate>,
    Query(knobs): Query<SolveKnobs>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let custom_profiles = tenant
        .db
        .with_conn(move |conn| {
            lineup_db::solver_profile::SolverProfile::list_for_team(conn, team_id)
        })
        .await
        .map_err(internal_error)?;

    let profile_names: Vec<(String, Option<String>)> = custom_profiles
        .iter()
        .map(|p| (p.name.clone(), p.description.clone()))
        .collect();

    Ok(Html(
        templates::solve::preset_bar(date, &knobs, &profile_names).into_string(),
    ))
}

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
        top_boat_stacking_weight: p.top_boat_stacking_weight,
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

/// Override walk-on rowers' availability to "Yes" so the solver
/// includes them. Transient — no DB write.
fn apply_walkons(snapshot: &mut DbSnapshot, knobs: &SolveKnobs) {
    use lineup_db::availability::types::AvailabilityStatus;
    use lineup_db::rower::types::RowerId;

    for id_str in &knobs.walkon {
        if let Ok(id) = id_str.parse::<RowerId>() {
            snapshot.availability.insert(id, AvailabilityStatus::Yes);
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
        let was_pinned_seats: std::collections::HashSet<(lineup_db::rower::types::RowerId, BoatId, i32)> =
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
        return Ok(super::maybe_page_authed(
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
    Ok(super::maybe_page_authed(
        &format!("Set Lineups · {date}"),
        content,
        hx,
        &tenant,
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

// =====================================================================
// Editor partial — re-renders the lineup editor from explicit placements
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct EditorParams {
    #[serde(default)]
    seat: Vec<String>,
    #[serde(default)]
    boat: Vec<String>,
    #[serde(default)]
    lock: Vec<String>,
    #[serde(default)]
    walkon: Vec<String>,
    #[serde(default)]
    no_show: Vec<String>,
    #[serde(default)]
    pin: Vec<String>,
    #[serde(default)]
    was_pin: Vec<String>,
    #[serde(default)]
    boat_pin: Vec<String>,
    #[serde(default)]
    boat_was_pin: Vec<String>,
    #[serde(default)]
    boat_lock: Vec<String>,
}

/// `GET /solve/{date}/editor` — re-render the lineup editor from the
/// given placement state. No solver run — just snapshot lookup + template.
/// Used by the Alpine component after each client-side operation.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn editor_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(date): Path<NaiveDate>,
    Query(params): Query<EditorParams>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let mut snapshot = tenant
        .db
        .with_conn(move |conn| DbSnapshot::for_team_date(conn, team_id, date))
        .await
        .map_err(internal_error)?;

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
    let active_boats: std::collections::HashSet<BoatId> = params.boat.iter()
        .filter_map(|s| s.parse::<BoatId>().ok())
        .collect();

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
        templates::solve::lineup_editor(&snapshot, date, &editor, &flags, &unavailable, &walkon_ids).into_string(),
    ))
}

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
    #[serde(default)]
    description: Option<String>,
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
    if name.is_empty() || SolverConfig::is_builtin(&name) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Resolve the config from the preset (built-in or default).
    let config = SolverConfig::from_preset(&input.preset).unwrap_or_default();

    let description = input.description.filter(|d| !d.trim().is_empty());
    let new_profile = lineup_db::solver_profile::NewSolverProfile {
        team_id,
        name,
        description,
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
        top_boat_stacking_weight: config.top_boat_stacking_weight,
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

/// `DELETE /solver-profile/{name}` — delete a custom profile.
/// Built-in presets cannot be deleted. Returns 200 on success
/// (HTMX reloads the page).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn delete_profile_handler(
    jar: CookieJar,
    Extension(tenant): Extension<crate::state::TenantContext>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    if name.is_empty() || SolverConfig::is_builtin(&name) {
        return Err(StatusCode::BAD_REQUEST);
    }

    tenant
        .db
        .with_conn(move |conn| {
            use lineup_db::solver_profile::SolverProfile;
            if let Some(profile) = SolverProfile::find_by_name(conn, team_id, &name)? {
                SolverProfile::delete(conn, profile.id)?;
            }
            Ok(())
        })
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::OK)
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
