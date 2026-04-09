//! Milestone 2: hard-constraints-only Pumpkin model.
//!
//! Takes a `DbSnapshot` and a list of requested boats, produces a feasible
//! seat assignment (one per requested boat) or declares infeasibility. There
//! is no objective function yet — any satisfying assignment is returned. The
//! search is driven by the "every seat of every requested boat must be
//! filled" constraint, which prevents the trivial all-zero solution.
//!
//! Variable encoding: `x[rower_idx][boat_idx][seat] ∈ {0,1}`. A variable is
//! only created for combinations where the rower is eligible for that seat
//! (cox seat → `can_cox`; other seats → any available rower). Ineligible
//! combinations simply don't exist in the model, which is cleaner than
//! creating vars with domain `{0}`.

mod decode;
mod model;
mod soft_fleet;
mod soft_seats;

use anyhow::{anyhow, bail, Result};
use chrono::NaiveDate;
use lineup_db::boat::{types::BoatId, Boat};
use lineup_db::rower::{types::RowerId, Rower};
use lineup_db::snapshot::DbSnapshot;
use pumpkin_conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_core::branching::Brancher;
use pumpkin_core::conflict_resolving::ConflictResolver;
use pumpkin_core::optimisation::linear_sat_unsat::LinearSatUnsat;
use pumpkin_core::optimisation::solution_callback::SolutionCallback;
use pumpkin_core::optimisation::OptimisationDirection;
use pumpkin_core::results::{OptimisationResult, ProblemSolution, SolutionReference};
use pumpkin_core::termination::{Indefinite, TimeBudget};
use pumpkin_core::variables::TransformableVariable;
use pumpkin_core::Solver;
use std::ops::ControlFlow;

use decode::decode_solution;
use model::ModelBuilder;

#[derive(Debug, Clone)]
pub struct SolveRequest {
    pub date: NaiveDate,
    /// Fleet the solver may *consider* fielding today. The solver chooses
    /// which of these to actually use via per-boat `use[b]` binary
    /// decision variables, driven by S8 (maximise rowers placed) and the
    /// weight-class / skill trade-offs. IDs must refer to entries in
    /// `snapshot.sweep_boats`. An empty list means "use every in-service
    /// sweep boat as a candidate".
    ///
    /// Primitive coach-override semantics: to require a specific boat,
    /// pass it alone; to forbid a boat, just don't include it.
    pub boats: Vec<BoatId>,
    /// Whether and how aggressively the solver may partial-fill a boat
    /// (leave specific "optional" seats empty even when the boat is
    /// fielded). Default `Strict`: no partial fills, every seat of every
    /// fielded boat must be filled. See `PartialFillPolicy` for details.
    pub partial_fill: PartialFillPolicy,
    /// S7 novelty factor. Controls how aggressively the solver avoids
    /// lineups that resemble historical committed lineups.
    ///
    /// - `0`: no novelty enforcement (exact repeats are fine).
    /// - `1`: deprioritize lineups that are 1 seat (or fewer) different
    ///   from a historical lineup. Penalises exact repeats harder than
    ///   "all but 1 seat same", but both incur a cost.
    /// - `2`: extends the penalty band to 2-seat differences.
    /// - Higher values widen the band and steepen the per-distance
    ///   penalty.
    ///
    /// Encoded as a per-historical-lineup soft constraint (see the S7
    /// block in `solve`). See `crates/solver/README.md` §S7 for the
    /// full formula.
    pub novelty_factor: i32,
    /// Wall-clock budget the solver may spend looking for an optimal
    /// assignment. `None` lets the solver run to proven optimality
    /// (`Indefinite`), which is fine for small instances but can take
    /// minutes on a full club fleet. For interactive use, set this to
    /// a few seconds — the solver returns best-found-so-far with
    /// `SolveStatus::Timeout` if the budget expires before optimality
    /// is proven.
    pub time_budget: Option<std::time::Duration>,
    /// Per-constraint weights controlling how strongly each soft
    /// constraint contributes to the objective. See [`SolverConfig`]
    /// for details. Default values preserve the historical behaviour
    /// (mostly 1, cox cooldown = 5).
    pub config: SolverConfig,
}

/// Per-constraint weight multipliers controlling how strongly each
/// soft constraint contributes to the objective function. Every
/// soft constraint scales its per-unit penalty (or reward) by the
/// corresponding field here before pushing the term into the
/// minimisation objective.
///
/// **Zero disables.** Setting any weight to `0` skips the entire
/// constraint block — no auxiliary variables created, no linking
/// constraints posted, no obj_terms pushed. This is both the
/// performance-friendly path (Pumpkin would panic on `.scaled(0)`
/// anyway) and the semantic way to turn a constraint off for a
/// particular solve.
///
/// **Negative values invert the constraint.** A negative
/// `skill_variance_weight` would reward high skill spread, a
/// negative `placement_reward_weight` would penalise fielding
/// boats, etc. This is a footgun but occasionally useful for
/// experiments — the type system doesn't forbid it, but stick to
/// non-negative values for normal coach use.
///
/// **Constraints with per-entity weights** (S2 / S3 stored pair and
/// seat affinity weights, S4 `side_strength`, S8 `seats_total`)
/// multiply their per-entity weight by the global config
/// multiplier. So `SolverConfig::pair_affinity_weight = 2` would
/// double the effect of every stored `pair_affinity.weight`, not
/// replace it.
///
/// **Future work.** A TOML config file loader that lets a coach
/// tune these weights per-club without recompiling. Deferred until
/// there's a concrete admin UI or CLI flag surface that calls for
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolverConfig {
    /// S1 skill variance penalty per unit of `max_skill - min_skill`
    /// within a boat. Default **1**.
    pub skill_variance_weight: i32,
    /// S2 pair-affinity multiplier. Scales each stored
    /// `pair_affinity.weight` when the pair appears in the same
    /// 2-seat partition. Default **1** (use stored weight as-is).
    pub pair_affinity_weight: i32,
    /// S3 seat-affinity multiplier. Scales each stored
    /// `rower_seat_affinity.weight`. Default **1**.
    pub seat_affinity_weight: i32,
    /// S4 side-preference multiplier. Scales each rower's
    /// `side_strength` when they're placed on the wrong side.
    /// Default **1**.
    pub side_preference_weight: i32,
    /// S5 weight-class slack multiplier. Scales both the `over` and
    /// `under` slack variables per boat. Default **1**.
    pub weight_class_slack_weight: i32,
    /// S6 cox-cooldown penalty. Flat constant added to the
    /// objective when a non-designated cox is placed as cox within
    /// the cooldown window. Default **5** — roughly the same
    /// ballpark as a strongly side-locked rower on the wrong side
    /// under S4.
    pub cox_cooldown_penalty: i32,
    /// S7 novelty multiplier. Scales the per-historical-lineup
    /// similarity penalty. Default **1**. Note that the *width* of
    /// the penalty band is controlled separately by
    /// `SolveRequest.novelty_factor`.
    pub novelty_weight: i32,
    /// S8 placement-reward multiplier. Scales the per-boat
    /// `-seats_total` reward for fielding a boat. Default **1**.
    pub placement_reward_weight: i32,
    /// S9 pair-strength multiplier. Scales the per-partition
    /// `pair_max - pair_min` penalty. Default **1**.
    pub pair_strength_weight: i32,
    /// S9b extra bow-pair strength-balance weight. The bow pair
    /// (seats 1 and 2) has an outsized effect on set and steering —
    /// a strength mismatch there is more visible and more costly
    /// than in the engine room or the stern pair. We encode this by
    /// pushing an *additional* `diff.scaled(bow_pair_strength_weight)`
    /// term on top of the standard S9 per-partition term whenever
    /// the partition is (1, 2). Total effective weight on the bow
    /// partition becomes `pair_strength_weight + bow_pair_strength_weight`.
    /// Default **2** (bow pair effectively costs 3× a regular pair's
    /// strength diff under the default `pair_strength_weight = 1`).
    pub bow_pair_strength_weight: i32,
    /// S10 pair-height-balance multiplier. Scales the per-partition
    /// `height_max - height_min` penalty so the solver tries to keep
    /// similarly-heighted rowers together in a pair. Intentionally
    /// light — mixed-height pairs row fine, this is just a gentle
    /// preference. Default **1**.
    pub height_balance_weight: i32,
    /// S11 end-pair skill-reward multiplier. 8-boats only. Rewards
    /// placing high-skill rowers in the end pairs (seats 1, 2, 7, 8)
    /// of an eight: the bow pair sets the rhythm for the rest of the
    /// crew and the stern pair leads the boat through the stroke.
    /// Encoded as a negative-coefficient term on each end-pair
    /// `seat_skill` var, i.e. the solver *maximises* end-pair skill
    /// ordinals to minimise the overall objective. Default **1**.
    pub end_pair_skill_weight: i32,
    /// S12 engine-room strength-reward multiplier. 8-boats only.
    /// Rewards placing strong rowers in the middle four seats
    /// (3, 4, 5, 6) — the "engine room" that provides the bulk of
    /// an eight's propulsive power. Same negative-coefficient
    /// encoding as S11 but over `seat_strength` vars. Default **1**.
    pub engine_room_strength_weight: i32,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            skill_variance_weight: 1,
            pair_affinity_weight: 1,
            seat_affinity_weight: 1,
            side_preference_weight: 1,
            weight_class_slack_weight: 1,
            cox_cooldown_penalty: 5,
            novelty_weight: 1,
            placement_reward_weight: 1,
            pair_strength_weight: 1,
            bow_pair_strength_weight: 2,
            height_balance_weight: 1,
            end_pair_skill_weight: 1,
            engine_room_strength_weight: 1,
        }
    }
}

/// Policy for how aggressively the solver may leave designated "optional"
/// seats empty on boats it fields.
///
/// Some clubs prefer going out in a 7/8-filled 8+ (missing seat 3 or 4)
/// rather than downsizing to a smaller boat and benching more rowers.
/// This enum controls that trade-off.
///
/// The *set* of optional seats is hardcoded per boat class by
/// `optional_seats` — e.g. an 8+ has `[3, 4]` as optional, a 4-boat has
/// no optional seats (partial-filling a 4 is too structurally unbalanced
/// to be useful). The `Allowed(k)` variant sets the maximum number of
/// those optional seats that may be empty per boat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartialFillPolicy {
    /// No partial fills. Every seat of every fielded boat must be
    /// filled exactly once. This is the current/default behaviour.
    Strict,
    /// Each fielded boat may have up to `k` of its optional seats
    /// empty. For an 8+ with optional seats `[3, 4]`, `Allowed(1)`
    /// permits "missing seat 3" or "missing seat 4" but not both;
    /// `Allowed(2)` permits any combination including both empty.
    Allowed(i32),
}

impl Default for PartialFillPolicy {
    fn default() -> Self {
        Self::Strict
    }
}

impl PartialFillPolicy {
    fn max_empty(self) -> i32 {
        match self {
            Self::Strict => 0,
            Self::Allowed(k) => k.max(0),
        }
    }
}

/// S6 cox-cooldown window. Non-designated coxes who coxed within this
/// many days of the current practice date incur a penalty if the solver
/// tries to seat them as cox again. Designated coxswains are exempt.
/// 14 days ≈ two weeks of practice — covers the full rotation horizon
/// most clubs care about ("don't have the same person cox two weeks
/// running") without being so long it effectively locks out anyone
/// who ever coxes.
pub(crate) const COX_COOLDOWN_DAYS: i64 = 14;

// S6 cox-cooldown penalty and all other per-constraint weights are now
// controlled via `SolverConfig` on the SolveRequest — see the struct
// definition above. S7 novelty band width is separately controlled by
// `SolveRequest.novelty_factor`.

#[derive(Debug, Clone)]
pub struct SolveResult {
    pub status: SolveStatus,
    pub lineups: Vec<ProposedLineup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveStatus {
    Satisfied,
    Unsatisfiable,
    Timeout,
}

#[derive(Debug, Clone)]
pub struct ProposedLineup {
    pub boat_id: BoatId,
    pub boat_name: String,
    /// Did the solver choose to field this boat today? Boats with
    /// `used = false` have no seat assignments — they were candidates
    /// the solver rejected as suboptimal.
    pub used: bool,
    /// (seat_position, rower_id). Empty when `used = false`.
    /// `seat_position = 0` is the cox seat on coxed boats; `1..=seat_count`
    /// are the rowing seats (bow → stroke).
    pub seats: Vec<(i32, RowerId)>,
}

// The former `greedy_fleet_selection` helper was removed when boat
// selection moved inside the Pumpkin model (see S8). Candidate boats
// are now passed wholesale to `solve`, and the solver decides which to
// field via `use[b]` decision variables balanced against S1/S5/S8.

/// Find a feasible seat assignment for the requested boats. Hard constraints
/// only; no objective function.
#[tracing::instrument(level = "debug", skip_all, fields(date = %request.date, n_boats = request.boats.len()), err)]
pub fn solve(snapshot: &DbSnapshot, request: &SolveRequest) -> Result<SolveResult> {
    // Resolve candidate fleet. An empty `request.boats` means "consider
    // every in-service sweep boat".
    let boats: Vec<&Boat> = if request.boats.is_empty() {
        snapshot.sweep_boats.iter().collect()
    } else {
        request
            .boats
            .iter()
            .map(|bid| {
                snapshot
                    .sweep_boats
                    .iter()
                    .find(|b| b.id == *bid)
                    .ok_or_else(|| anyhow!("boat {} not in snapshot sweep fleet", bid))
            })
            .collect::<Result<_>>()?
    };

    if boats.is_empty() {
        return Ok(SolveResult {
            status: SolveStatus::Satisfied,
            lineups: vec![],
        });
    }

    let available: Vec<&Rower> = snapshot.available_rowers().collect();

    if available.is_empty() {
        bail!("no rowers are available for sweep seating on {}", request.date);
    }

    // All mutable solver state lives on a single `ModelBuilder`. Its
    // methods post whole constraint blocks (variable creation, H1,
    // H2, H5/S5, H6) and the remaining blocks below access shared
    // fields directly. Per-constraint weights, the candidate fleet,
    // and the available-rowers list all move onto the builder.
    let mut m = ModelBuilder::new(boats, available, request.config);

    // S8 — per-boat placement reward (`-seats_total · use[b]`).
    // Posted up front so the fleet-selection objective is visible to
    // later constraint propagation, though order only affects
    // reporting — the objective sum is commutative.
    m.post_s8_placement_reward();

    // --- Variables ---
    // Create one x[(r, b, s)] ∈ {0, 1} per eligible triple and, along
    // the way, collect wrong-side candidates into
    // `m.wrong_side_by_rower` for S4 to consume. See
    // `ModelBuilder::create_variables` for the full eligibility
    // rules and per-rower aggregation rationale.
    m.create_variables();

    // --- Fleet-level soft constraints: S4 wrong-side aggregation,
    // S6 cox cooldown, S7 novelty vs recent lineups. These all
    // live in `soft_fleet.rs` as ModelBuilder methods. S8 already
    // ran above because it only needs `use_b` / `boats` / `cfg`
    // and doesn't touch `x`.
    m.post_s4_wrong_side()?;
    m.post_s6_cox_cooldown(snapshot, request.date)?;
    m.post_s7_novelty(snapshot, request.novelty_factor)?;

    // --- Hard constraints: H1 seat fill + partial-fill cap, H2
    // rower-at-most-one, H6 fleet capacity, H5 weight-class wall
    // (plus the S5 weight-class slack bundled with it because the
    // wall and the slack iterate the same boat loop and share the
    // `positive_terms` sum). See the method docs on ModelBuilder
    // for the per-constraint rationale.
    m.post_h1_seat_fill(request.partial_fill)?;
    m.post_h2_at_most_one()?;
    m.post_h6_fleet_capacity()?;
    m.post_h5_s5_weight_class()?;

    // --- Seat-level soft constraints ---
    //
    // First build the shared `seat_{skill,strength,height}_by_seat`
    // maps for any trait whose consumer constraints are enabled,
    // then post the per-boat / per-partition / per-end-pair soft
    // terms that read them. All of this lives in `soft_seats.rs`.
    m.build_seat_trait_maps()?;

    m.post_s1_skill_variance()?;
    m.post_s2_pair_affinities(snapshot)?;
    m.post_s3_seat_affinities(snapshot)?;
    m.post_s9_pair_strength()?;
    m.post_s10_pair_height()?;
    m.post_s11_end_pair_skill();
    m.post_s12_engine_room_strength();

    // --- Objective variable ---
    // Sum of every weighted term pushed into `m.obj_terms` by the
    // soft constraint blocks above. A generous domain is fine —
    // Pumpkin propagates tighter bounds from the term domains
    // during search. The lower bound must be negative because S2
    // pair-affinity / S3 seat-affinity / S11 / S12 rewards
    // contribute negative terms.
    let objective = m.solver.new_bounded_integer(-10_000, 10_000);
    if !m.obj_terms.is_empty() {
        let mut link_terms = m.obj_terms.clone();
        link_terms.push(objective.scaled(-1));
        let tag = m.solver.new_constraint_tag();
        m.solver
            .add_constraint(pumpkin_constraints::equals(link_terms, 0, tag))
            .post()
            .map_err(|e| anyhow!("objective link: {e:?}"))?;
    }

    // --- Solve (optimisation) ---
    let mut brancher = m.solver.default_brancher();
    let mut resolver = ResolutionResolver::default();

    // The termination type depends on whether the caller set a time
    // budget. Both branches return the same `OptimisationResult<()>`
    // (NoCallback fixes `Callback::Stop = ()`), so we can unify the
    // result before the outer match.
    let opt_result = match request.time_budget {
        None => {
            let mut termination = Indefinite;
            let procedure =
                LinearSatUnsat::new(OptimisationDirection::Minimise, objective, NoCallback);
            m.solver
                .optimise(&mut brancher, &mut termination, &mut resolver, procedure)
        }
        Some(budget) => {
            let mut termination = TimeBudget::starting_now(budget);
            let procedure =
                LinearSatUnsat::new(OptimisationDirection::Minimise, objective, NoCallback);
            m.solver
                .optimise(&mut brancher, &mut termination, &mut resolver, procedure)
        }
    };

    let result: SolveResult = match opt_result {
        OptimisationResult::Optimal(sol) | OptimisationResult::Satisfiable(sol) => {
            let lineups = decode_solution(&m.x, &m.use_b, &m.boats, &m.available, |v| {
                sol.get_integer_value(v)
            });
            SolveResult {
                status: SolveStatus::Satisfied,
                lineups,
            }
        }
        OptimisationResult::Stopped(sol, _) => {
            let lineups = decode_solution(&m.x, &m.use_b, &m.boats, &m.available, |v| {
                sol.get_integer_value(v)
            });
            SolveResult {
                status: SolveStatus::Satisfied,
                lineups,
            }
        }
        OptimisationResult::Unsatisfiable => SolveResult {
            status: SolveStatus::Unsatisfiable,
            lineups: vec![],
        },
        OptimisationResult::Unknown => SolveResult {
            status: SolveStatus::Timeout,
            lineups: vec![],
        },
    };
    Ok(result)
}

/// No-op solution callback for `LinearSatUnsat`. We don't need intermediate
/// solution hooks right now, but the `LinearSatUnsat::new` API requires
/// *some* callback. When we add top-N alternative lineups via tabu
/// re-solve this will grow into a real callback that records every
/// improving solution.
#[derive(Debug, Default)]
struct NoCallback;

impl<B: Brancher, R: ConflictResolver> SolutionCallback<B, R> for NoCallback {
    type Stop = ();

    fn on_solution_callback(
        &mut self,
        _solver: &Solver,
        _solution: SolutionReference<'_>,
        _brancher: &B,
        _resolver: &R,
    ) -> ControlFlow<Self::Stop> {
        ControlFlow::Continue(())
    }
}

