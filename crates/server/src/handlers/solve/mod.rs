//! `GET /solve/{date}` — run the solver for a date and render the
//! proposed lineup, plus `POST /commit/{date}` to persist the primary.
//!
//! Solver parameters are exposed to the coach as a [`SolveKnobs`] form
//! on the solve view. The view extracts them from the URL query string
//! so a particular run is bookmarkable; the commit form re-sends them
//! as hidden fields so the persisted lineup matches the one the coach
//! actually saw.

mod commit;
mod editor;
mod profiles;
mod stream;
mod view;

pub(crate) use commit::{commit_handler, commit_lineup_handler};
pub(crate) use editor::editor_handler;
pub(crate) use profiles::{
    delete_profile_handler, edit_profile_handler, preset_bar_handler, save_profile_handler,
};
pub(crate) use stream::stream_handler;
pub(crate) use view::view_handler;

use std::time::Duration;

use chrono::NaiveDate;
use lineup_db::{
    boat::types::BoatId,
    lineup::{Lineup, SeatPosition},
    practice::{Practice, PracticeId},
    rower::types::RowerId,
    snapshot::DbSnapshot,
};
use lineup_solver::{solve, PartialFillPolicy, SolveRequest, SolveResult, SolverConfig};
use serde::{Deserialize, Serialize};
use tokio::sync::OwnedSemaphorePermit;

use crate::{
    handlers::{internal_error, ErrorResponse},
    state::SolverCtx,
};

/// A rower-boat-seat triple, serialized as "rower_id:boat_id:seat" in
/// URL query params and form fields.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SeatTriple {
    pub(crate) rower_id: RowerId,
    pub(crate) boat_id: BoatId,
    pub(crate) seat: SeatPosition,
}

impl std::fmt::Display for SeatTriple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.rower_id, self.boat_id, self.seat)
    }
}

impl std::str::FromStr for SeatTriple {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        let mut parts = s.splitn(3, ':');
        let rower_id = parts.next().ok_or(())?.parse().map_err(|_| ())?;
        let boat_id = parts.next().ok_or(())?.parse().map_err(|_| ())?;
        let seat = parts.next().ok_or(())?.parse().map_err(|_| ())?;
        Ok(Self {
            rower_id,
            boat_id,
            seat,
        })
    }
}

impl Serialize for SeatTriple {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SeatTriple {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse()
            .map_err(|_| serde::de::Error::custom("expected rower_id:boat_id:seat"))
    }
}

/// A source-dest boat pair for seat transfers, serialized as "src:dest".
#[derive(Debug, Clone)]
pub(crate) struct BoatPair {
    pub(crate) source: BoatId,
    pub(crate) dest: BoatId,
}

impl std::str::FromStr for BoatPair {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        let (a, b) = s.split_once(':').ok_or(())?;
        Ok(Self {
            source: a.parse().map_err(|_| ())?,
            dest: b.parse().map_err(|_| ())?,
        })
    }
}

impl Serialize for BoatPair {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}:{}", self.source, self.dest))
    }
}

impl<'de> Deserialize<'de> for BoatPair {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse()
            .map_err(|_| serde::de::Error::custom("expected source_boat:dest_boat"))
    }
}

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
const DEFAULT_BUDGET_SECS: u64 = 5;

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
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    pub(crate) no_show: Vec<RowerId>,
    /// When present and non-zero, triggers the solver. Without this,
    /// the solve page shows knobs + existing lineups but doesn't run
    /// the solver — the coach clicks "Generate" to trigger it.
    #[serde(default)]
    pub(crate) generate: i32,
    /// Seat locks (explicit coach locks). Each value is
    /// `rower_id:boat_id:seat_position`.
    #[serde(default)]
    pub(crate) lock: Vec<SeatTriple>,
    /// Dirty pins (manual edits since last generate). Same format.
    /// Honored as solver constraints on Generate, then converted
    /// to `was_pin` in the response.
    #[serde(default)]
    pub(crate) pin: Vec<SeatTriple>,
    /// Was-pinned seats (honored last generate, no longer constrained).
    /// Coach can promote to lock via the icon.
    #[serde(default)]
    pub(crate) was_pin: Vec<SeatTriple>,
    /// Solver preset name. One of: balanced, even_speed, tiered, random.
    /// Overrides the default SolverConfig when present.
    #[serde(default)]
    pub(crate) preset: String,
    /// Walk-on rower IDs. Rowers who showed up without having set
    /// availability. Their availability is transiently overridden to
    /// "Yes" for this solve session (no DB write). Deserialized from
    /// repeated `walkon=<rower_id>` query params.
    #[serde(default)]
    pub(crate) walkon: Vec<RowerId>,
    /// Active boat IDs from the editor. When non-empty, restricts the
    /// solver to only consider these boats. Deserialized from repeated
    /// `boat=<boat_id>` query params injected by the editor JS.
    #[serde(default)]
    pub(crate) boat: Vec<BoatId>,
    /// Dirty-pinned boats (must be fielded this generate).
    #[serde(default)]
    pub(crate) boat_pin: Vec<BoatId>,
    /// Was-pinned boats (honored last generate, free next time).
    #[serde(default)]
    pub(crate) boat_was_pin: Vec<BoatId>,
    /// Locked boats (always fielded).
    #[serde(default)]
    pub(crate) boat_lock: Vec<BoatId>,
    /// Pre-populated seat placements for the editor. Each value is
    /// `rower_id:boat_id:seat_position`. Used when navigating from the
    /// history page "Edit lineup" button.
    #[serde(default)]
    pub(crate) seat: Vec<SeatTriple>,
    /// Transfer rowers from one boat to another. Format: "source_boat_id:dest_boat_id".
    /// The server maps seats by position (stern->stern, bow->bow, cox->cox).
    #[serde(default)]
    pub(crate) transfer: Option<BoatPair>,
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
            transfer: None,
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
    pub(super) fn to_request(
        &self,
        date: NaiveDate,
        snapshot: &DbSnapshot,
        baselines: Vec<lineup_solver::ReferenceLineup>,
    ) -> SolveRequest {
        use lineup_solver::{ReferenceLineup, ReferencePlacement};
        use std::collections::BTreeMap;

        let novelty_weight = self.novelty.max(0);
        let mut reference_lineups = Vec::new();

        if novelty_weight > 0 {
            let mut groups: BTreeMap<
                (NaiveDate, lineup_db::boat::types::BoatId),
                Vec<ReferencePlacement>,
            > = BTreeMap::new();
            for p in &snapshot.recent_placements {
                if p.is_cox || p.seat_position.as_int() == 0 {
                    continue;
                }
                groups
                    .entry((p.practice_date, p.boat_id))
                    .or_default()
                    .push(ReferencePlacement {
                        rower_id: p.rower_id,
                        boat_id: p.boat_id,
                        seat: p.seat_position.as_int(),
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
        let mut boat_set: std::collections::HashSet<BoatId> = self.boat.iter().copied().collect();
        for entries in [&self.lock, &self.pin] {
            for triple in entries.iter() {
                boat_set.insert(triple.boat_id);
            }
        }
        for entries in [&self.boat_pin, &self.boat_lock] {
            for bid in entries.iter() {
                boat_set.insert(*bid);
            }
        }
        let boats: Vec<BoatId> = boat_set.into_iter().collect();
        // Required boats: dirty-pinned + locked boats must be fielded.
        let mut required_boats: Vec<BoatId> = self
            .boat_pin
            .iter()
            .chain(self.boat_lock.iter())
            .copied()
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
            sa_postprocess: true,
        }
    }

    /// Resolve the SolverConfig from the preset name. Built-in presets
    /// are checked first; custom profiles are resolved later in the
    /// handler (requires DB access).
    pub(super) fn resolve_config(&self) -> SolverConfig {
        SolverConfig::from_preset(&self.preset).unwrap_or_default()
    }

    pub(super) fn parse_locks(&self) -> Vec<lineup_solver::SeatLock> {
        self.lock
            .iter()
            .map(|t| lineup_solver::SeatLock {
                rower_id: t.rower_id,
                boat_id: t.boat_id,
                seat: t.seat.as_int(),
            })
            .collect()
    }

    /// Convert a slice of `SeatTriple`s to the `(RowerId, BoatId, i32)` set
    /// used by display flags.
    pub(super) fn triples_to_set(
        entries: &[SeatTriple],
    ) -> std::collections::HashSet<(RowerId, BoatId, i32)> {
        entries
            .iter()
            .map(|t| (t.rower_id, t.boat_id, t.seat.as_int()))
            .collect()
    }

    /// Convert a slice of `BoatId`s to a `HashSet`.
    pub(super) fn boat_id_set(entries: &[BoatId]) -> std::collections::HashSet<BoatId> {
        entries.iter().copied().collect()
    }
}

/// Build baseline reference lineups from the `based_on` practice IDs in the
/// knobs. Each checked practice's committed lineup becomes one
/// `ReferenceLineup` with negative weight (prefer similarity).
pub(super) async fn build_baselines(
    knobs: &SolveKnobs,
    db: &lineup_db::state::Db,
    _team_id: lineup_db::team::TeamId,
) -> Result<Vec<lineup_solver::ReferenceLineup>, ErrorResponse> {
    use lineup_solver::{ReferenceLineup, ReferencePlacement};

    let similarity = knobs.similarity.max(0);
    if similarity == 0 || knobs.based_on.is_empty() {
        return Ok(vec![]);
    }

    let practice_ids: Vec<PracticeId> = knobs
        .based_on
        .iter()
        .filter_map(|s| s.parse::<PracticeId>().ok())
        .collect();
    if practice_ids.is_empty() {
        return Ok(vec![]);
    }

    let refs = db
        .with_conn(move |conn| {
            let mut refs = Vec::new();
            for pid in &practice_ids {
                let Some(_practice) = Practice::get(conn, *pid)? else {
                    continue;
                };
                let committed = Lineup::for_practice(conn, *pid)?;
                let placements: Vec<ReferencePlacement> = committed
                    .iter()
                    .flat_map(|c| {
                        c.seats.iter().map(|s| ReferencePlacement {
                            rower_id: s.rower_id,
                            boat_id: c.lineup.boat_id,
                            seat: s.seat_position.as_int(),
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
pub(super) fn apply_no_shows(snapshot: &mut DbSnapshot, knobs: &SolveKnobs) {
    use lineup_db::availability::types::AvailabilityStatus;

    for &id in &knobs.no_show {
        snapshot.availability.insert(id, AvailabilityStatus::No);
    }
}

/// Override walk-on rowers' availability to "Yes" so the solver
/// includes them. Transient — no DB write.
pub(super) fn apply_walkons(snapshot: &mut DbSnapshot, knobs: &SolveKnobs) {
    use lineup_db::availability::types::AvailabilityStatus;

    for &id in &knobs.walkon {
        snapshot.availability.insert(id, AvailabilityStatus::Yes);
    }
}

pub(super) fn profile_to_config(p: &lineup_db::solver_profile::SolverProfile) -> SolverConfig {
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
        pair_eligibility_weight: p.pair_eligibility_weight,
        minimize_bench_weight: p.minimize_bench_weight,
        boat_size_stacking_weight: p.boat_size_stacking_weight,
        bench_cooldown_penalty: p.bench_cooldown_penalty,
        stroke_spread_weight: p.stroke_spread_weight,
        eight_bias: p.eight_bias,
        coxed_four_bias: p.coxed_four_bias,
        four_bias: p.four_bias,
        quad_bias: p.quad_bias,
        pair_bias: p.pair_bias,
        double_bias: p.double_bias,
        single_bias: p.single_bias,
    }
}

/// Map rowers from one boat to another by seat position.
/// Stroke→stroke, bow→bow, cox→cox (if dest has cox).
/// Rowers that don't fit (downsizing) are dropped (go to bench).
/// If rigging differs, pairs are swapped so rowers stay on their side.
pub(super) fn map_transfer_seats(
    src_seats: &std::collections::HashMap<i32, lineup_db::rower::types::RowerId>,
    src: &lineup_db::boat::Boat,
    dst: &lineup_db::boat::Boat,
) -> std::collections::HashMap<i32, lineup_db::rower::types::RowerId> {
    let mut result = std::collections::HashMap::new();
    let dst_has_cox = dst.has_cox.as_bool();
    let dst_count = dst.seat_count.as_int();
    let src_count = src.seat_count.as_int();

    // Cox → cox (if dest has cox).
    if dst_has_cox {
        if let Some(&cox) = src_seats.get(&0) {
            result.insert(0, cox);
        }
    }

    // Numbered seats: map by relative position. Process priority
    // seats first (stroke, bow, adjacent) so they claim their
    // mapped positions before mid-seats.
    let priority_order: Vec<i32> = {
        let mut order = Vec::new();
        // Stroke first (most important positional mapping)
        order.push(src_count);
        // Bow
        if src_count > 1 {
            order.push(1);
        }
        // Below stroke
        if src_count > 2 {
            order.push(src_count - 1);
        }
        // Above bow
        if src_count > 2 && !order.contains(&2) {
            order.push(2);
        }
        // Remaining middle seats
        for s in 1..=src_count {
            if !order.contains(&s) {
                order.push(s);
            }
        }
        order
    };
    for src_pos in priority_order {
        let Some(&rower) = src_seats.get(&src_pos) else {
            continue;
        };
        let dst_pos = if src_pos == src_count {
            dst_count // stroke → stroke
        } else if src_pos == 1 {
            1 // bow → bow
        } else if src_pos == src_count - 1 && dst_count > 2 {
            dst_count - 1 // below stroke → below stroke
        } else if src_pos == 2 && dst_count > 2 {
            2 // above bow → above bow
        } else if src_pos <= dst_count {
            src_pos // same position if it fits
        } else {
            continue; // doesn't fit → bench
        };
        if dst_pos >= 1 && dst_pos <= dst_count && !result.contains_key(&dst_pos) {
            result.insert(dst_pos, rower);
        }
    }

    // If rigging differs, swap rowers within each pair.
    if src.stroke_side != dst.stroke_side {
        let mut swapped = result.clone();
        let mut pos = dst_count;
        while pos >= 2 {
            let high = swapped.remove(&pos);
            let low = swapped.remove(&(pos - 1));
            if let Some(r) = high {
                swapped.insert(pos - 1, r);
            }
            if let Some(r) = low {
                swapped.insert(pos, r);
            }
            pos -= 2;
        }
        result = swapped;
    }

    result
}

fn default_alts() -> usize {
    DEFAULT_ALTS
}

fn default_budget() -> u64 {
    DEFAULT_BUDGET_SECS
}

/// Dispatch `solve()` onto the dedicated rayon thread pool via a
/// oneshot channel. The async handler awaits the result without
/// touching tokio's blocking pool — DB queries via deadpool-diesel
/// are unaffected regardless of how many concurrent solves are
/// in flight.
pub(super) async fn run_solve(
    solver: &SolverCtx,
    snapshot: DbSnapshot,
    request: SolveRequest,
) -> Result<SolveResult, ErrorResponse> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    solver.solver_pool.spawn(move || {
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
pub(super) async fn acquire_solve_permit(
    solver: &SolverCtx,
) -> Result<OwnedSemaphorePermit, ErrorResponse> {
    if let Ok(permit) = solver.solve_semaphore.clone().try_acquire_owned() {
        return Ok(permit);
    }
    let queue_start = std::time::Instant::now();
    tracing::info!(
        capacity = solver.solve_semaphore.available_permits(),
        "solve queued — semaphore at capacity"
    );
    let permit = solver
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

#[derive(Debug, Deserialize)]
pub(crate) struct EditorParams {
    #[serde(default)]
    pub(super) seat: Vec<SeatTriple>,
    #[serde(default)]
    pub(super) boat: Vec<BoatId>,
    #[serde(default)]
    pub(super) lock: Vec<SeatTriple>,
    #[serde(default)]
    pub(super) walkon: Vec<RowerId>,
    #[serde(default)]
    pub(super) no_show: Vec<RowerId>,
    #[serde(default)]
    pub(super) pin: Vec<SeatTriple>,
    #[serde(default)]
    pub(super) was_pin: Vec<SeatTriple>,
    #[serde(default)]
    pub(super) boat_pin: Vec<BoatId>,
    #[serde(default)]
    pub(super) boat_was_pin: Vec<BoatId>,
    #[serde(default)]
    pub(super) boat_lock: Vec<BoatId>,
    /// Transfer rowers from one boat to another. Format: "source_boat_id:dest_boat_id".
    /// The server maps seats by position (stern->stern, bow->bow, cox->cox).
    #[serde(default)]
    pub(super) transfer: Option<BoatPair>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DirectCommitInput {
    /// Repeated field: each value is "boat_id:seat_pos:rower_id".
    #[serde(default)]
    pub(super) seat: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SaveProfileInput {
    pub(super) name: String,
    pub(super) preset: String,
    #[serde(default)]
    pub(super) description: Option<String>,
    // Individual weight overrides — when present (from the modal form),
    // they override the basis preset's values.
    #[serde(default)]
    pub(super) skill_variance_weight: Option<i32>,
    #[serde(default)]
    pub(super) pair_affinity_weight: Option<i32>,
    #[serde(default)]
    pub(super) seat_affinity_weight: Option<i32>,
    #[serde(default)]
    pub(super) side_preference_weight: Option<i32>,
    #[serde(default)]
    pub(super) weight_class_slack_weight: Option<i32>,
    #[serde(default)]
    pub(super) cox_cooldown_penalty: Option<i32>,
    #[serde(default)]
    pub(super) placement_reward_weight: Option<i32>,
    #[serde(default)]
    pub(super) pair_strength_weight: Option<i32>,
    #[serde(default)]
    pub(super) bow_pair_strength_weight: Option<i32>,
    #[serde(default)]
    pub(super) height_balance_weight: Option<i32>,
    #[serde(default)]
    pub(super) end_pair_skill_weight: Option<i32>,
    #[serde(default)]
    pub(super) engine_room_strength_weight: Option<i32>,
    #[serde(default)]
    pub(super) partial_fill_bonus: Option<i32>,
    #[serde(default)]
    pub(super) non_scull_retention_weight: Option<i32>,
    #[serde(default)]
    pub(super) bow_cox_fit_weight: Option<i32>,
    #[serde(default)]
    pub(super) top_boat_stacking_weight: Option<i32>,
    #[serde(default)]
    pub(super) pair_eligibility_weight: Option<i32>,
    #[serde(default)]
    pub(super) minimize_bench_weight: Option<i32>,
    #[serde(default)]
    pub(super) boat_size_stacking_weight: Option<i32>,
    #[serde(default)]
    pub(super) bench_cooldown_penalty: Option<i32>,
    #[serde(default)]
    pub(super) stroke_spread_weight: Option<i32>,
    #[serde(default)]
    pub(super) eight_bias: Option<i32>,
    #[serde(default)]
    pub(super) coxed_four_bias: Option<i32>,
    #[serde(default)]
    pub(super) four_bias: Option<i32>,
    #[serde(default)]
    pub(super) quad_bias: Option<i32>,
    #[serde(default)]
    pub(super) pair_bias: Option<i32>,
    #[serde(default)]
    pub(super) double_bias: Option<i32>,
    #[serde(default)]
    pub(super) single_bias: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineup_db::boat::types::{CoxPosition, WeightClass};
    use lineup_db::boat::{types::BoatId, Boat};
    use lineup_db::rower::types::{RowerId, Side};
    use lineup_db::types::IntBool;
    use std::collections::HashMap;
    use test_case::test_case;

    fn boat(id: i32, seats: i32, has_cox: bool, stroke_side: Side) -> Boat {
        Boat {
            id: BoatId::new(id),
            name: format!("Boat{id}"),
            weight_class: WeightClass::Medium,
            seat_count: lineup_db::boat::types::SeatCount::new(seats),
            has_cox: IntBool::new(has_cox),
            oars_per_seat: lineup_db::boat::types::OarsPerSeat::new(1),
            acquired_at: None,
            manufactured_at: None,
            relinquished_at: None,
            stroke_side,
            cox_position: CoxPosition::Stern,
        }
    }

    fn seats(pairs: &[(i32, i32)]) -> HashMap<i32, RowerId> {
        pairs.iter().map(|&(s, r)| (s, RowerId::new(r))).collect()
    }

    // ── SeatTriple ──

    #[test]
    fn seat_triple_roundtrip() {
        let t = SeatTriple {
            rower_id: RowerId::new(10),
            boat_id: BoatId::new(20),
            seat: SeatPosition::new(3),
        };
        let s = t.to_string();
        assert_eq!(s, "10:20:3");
        let parsed: SeatTriple = s.parse().unwrap();
        assert_eq!(parsed, t);
    }

    #[test_case("" ; "empty string")]
    #[test_case("abc:2:3" ; "non-numeric rower")]
    #[test_case("1:2" ; "missing field")]
    #[test_case("only_one" ; "no colons")]
    fn seat_triple_rejects_invalid(input: &str) {
        assert!(input.parse::<SeatTriple>().is_err());
    }

    // ── triples_to_set ──

    #[test]
    fn triples_to_set_converts() {
        let triples = vec![SeatTriple {
            rower_id: RowerId::new(10),
            boat_id: BoatId::new(20),
            seat: SeatPosition::new(3),
        }];
        let set = SolveKnobs::triples_to_set(&triples);
        assert_eq!(set.len(), 1);
        assert!(set.contains(&(RowerId::new(10), BoatId::new(20), 3)));
    }

    // ── parse_locks ──

    #[test]
    fn parse_locks_valid() {
        let knobs = SolveKnobs {
            lock: vec![SeatTriple {
                rower_id: RowerId::new(10),
                boat_id: BoatId::new(20),
                seat: SeatPosition::new(3),
            }],
            ..Default::default()
        };
        let locks = knobs.parse_locks();
        assert_eq!(locks.len(), 1);
        assert_eq!(locks[0].rower_id, RowerId::new(10));
        assert_eq!(locks[0].boat_id, BoatId::new(20));
        assert_eq!(locks[0].seat, 3);
    }

    // ── boat_id_set ──

    #[test]
    fn boat_id_set_deduplicates() {
        let input = vec![BoatId::new(5), BoatId::new(5), BoatId::new(10)];
        let ids = SolveKnobs::boat_id_set(&input);
        assert_eq!(ids.len(), 2);
    }

    // ── SolveKnobs form round-trip ──

    /// Test that SolveKnobs deserializes from the query-string format
    /// that the browser actually sends (repeated keys for Vec fields).
    #[test]
    fn solve_knobs_deserializes_from_query_string() {
        let qs = "lock=1%3A2%3A3&lock=4%3A5%3A6&no_show=7&walkon=8&boat=10&boat=11&boat_lock=10&generate=1&budget=3";
        let knobs: SolveKnobs = serde_html_form::from_str(qs).unwrap();

        assert_eq!(knobs.lock.len(), 2);
        assert_eq!(knobs.lock[0].rower_id, RowerId::new(1));
        assert_eq!(knobs.lock[0].boat_id, BoatId::new(2));
        assert_eq!(knobs.lock[0].seat, SeatPosition::new(3));
        assert_eq!(knobs.lock[1].rower_id, RowerId::new(4));
        assert_eq!(knobs.no_show, vec![RowerId::new(7)]);
        assert_eq!(knobs.walkon, vec![RowerId::new(8)]);
        assert_eq!(knobs.boat, vec![BoatId::new(10), BoatId::new(11)]);
        assert_eq!(knobs.boat_lock, vec![BoatId::new(10)]);
        assert_eq!(knobs.generate, 1);
        assert_eq!(knobs.budget, 3);
    }

    // ── map_transfer_seats ──

    #[test]
    fn transfer_same_size_same_rig() {
        let src = boat(1, 8, true, Side::Port);
        let dst = boat(2, 8, true, Side::Port);
        let src_seats = seats(&[(0, 100), (1, 101), (8, 108)]);
        let result = map_transfer_seats(&src_seats, &src, &dst);
        assert_eq!(result[&0], RowerId::new(100)); // cox
        assert_eq!(result[&1], RowerId::new(101)); // bow
        assert_eq!(result[&8], RowerId::new(108)); // stroke
    }

    #[test]
    fn transfer_downsize_8_to_4() {
        let src = boat(1, 8, true, Side::Port);
        let dst = boat(2, 4, true, Side::Port);
        let src_seats = seats(&[
            (0, 100),
            (1, 101),
            (2, 102),
            (3, 103),
            (4, 104),
            (5, 105),
            (6, 106),
            (7, 107),
            (8, 108),
        ]);
        let result = map_transfer_seats(&src_seats, &src, &dst);
        assert_eq!(result[&0], RowerId::new(100)); // cox
        assert_eq!(result[&1], RowerId::new(101)); // bow → bow
        assert_eq!(result[&2], RowerId::new(102)); // above bow → above bow
        assert_eq!(result[&4], RowerId::new(108)); // stroke → stroke (priority)
        assert_eq!(result[&3], RowerId::new(107)); // below stroke → below stroke
        assert_eq!(result.len(), 5); // cox + 4 seats
    }

    #[test]
    fn transfer_cox_dropped_when_dest_coxless() {
        let src = boat(1, 4, true, Side::Port);
        let dst = boat(2, 4, false, Side::Port);
        let src_seats = seats(&[(0, 100), (1, 101), (4, 104)]);
        let result = map_transfer_seats(&src_seats, &src, &dst);
        assert!(!result.contains_key(&0)); // no cox seat in dest
        assert_eq!(result[&1], RowerId::new(101));
        assert_eq!(result[&4], RowerId::new(104));
    }

    #[test]
    fn transfer_rigging_swap_flips_pairs() {
        let src = boat(1, 4, false, Side::Port);
        let dst = boat(2, 4, false, Side::Starboard);
        // Seats 4,3 are stern pair; 2,1 are bow pair.
        let src_seats = seats(&[(1, 101), (2, 102), (3, 103), (4, 104)]);
        let result = map_transfer_seats(&src_seats, &src, &dst);
        // Pairs should flip: 4↔3, 2↔1
        assert_eq!(result[&3], RowerId::new(104));
        assert_eq!(result[&4], RowerId::new(103));
        assert_eq!(result[&1], RowerId::new(102));
        assert_eq!(result[&2], RowerId::new(101));
    }

    #[test]
    fn transfer_same_rig_no_flip() {
        let src = boat(1, 4, false, Side::Port);
        let dst = boat(2, 4, false, Side::Port);
        let src_seats = seats(&[(1, 101), (2, 102), (3, 103), (4, 104)]);
        let result = map_transfer_seats(&src_seats, &src, &dst);
        // Same rigging — no flip
        assert_eq!(result[&1], RowerId::new(101));
        assert_eq!(result[&4], RowerId::new(104));
    }

    #[test]
    fn transfer_empty_source() {
        let src = boat(1, 8, true, Side::Port);
        let dst = boat(2, 4, true, Side::Port);
        let result = map_transfer_seats(&HashMap::new(), &src, &dst);
        assert!(result.is_empty());
    }
}
