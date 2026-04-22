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
use pumpkin_core::variables::{AffineView, DomainId, TransformableVariable};
use pumpkin_core::Solver;
use std::collections::BTreeMap;
use std::ops::ControlFlow;

use decode::decode_solution;
use model::ModelBuilder;

/// A pre-pinned (rower, boat, seat) assignment the solver must
/// respect. The solver forces `x[r, b, s] = 1` and `use[b] = 1`.
#[derive(Debug, Clone)]
pub struct SeatLock {
    pub rower_id: RowerId,
    pub boat_id: BoatId,
    pub seat: i32,
}

pub struct SolveRequest {
    pub date: NaiveDate,
    /// Fleet the solver may *consider* fielding today. The solver chooses
    /// which of these to actually use via per-boat `use[b]` binary
    /// decision variables, driven by S8 (maximise rowers placed) and the
    /// weight-class / skill trade-offs. IDs must refer to entries in
    /// `snapshot.boats`. An empty list means "use every in-service
    /// boat as a candidate".
    ///
    /// Primitive coach-override semantics: to require a specific boat,
    /// pass it alone; to forbid a boat, just don't include it.
    pub boats: Vec<BoatId>,
    /// Whether and how aggressively the solver may partial-fill a boat
    /// (leave specific "optional" seats empty even when the boat is
    /// fielded). Default `Strict`: no partial fills, every seat of every
    /// fielded boat must be filled. See `PartialFillPolicy` for details.
    pub partial_fill: PartialFillPolicy,
    // novelty_factor removed — novelty is now expressed via
    // `reference_lineups` with positive weight. See the handler
    // for how recent placements are converted to references.
    /// Wall-clock budget the solver may spend looking for an optimal
    /// assignment. `None` lets the solver run to proven optimality
    /// (`Indefinite`), which is fine for small instances but can take
    /// minutes on a full club fleet. For interactive use, set this to
    /// a few seconds — the solver returns best-found-so-far with
    /// `SolveStatus::Timeout` if the budget expires before optimality
    /// is proven.
    ///
    /// **Per-alternative.** When `top_n > 1` this budget applies to
    /// *each* alternative independently. Total wall-clock time in
    /// the worst case is `top_n * time_budget`. If you want a tight
    /// global budget with multiple alternatives, shrink the per-call
    /// budget proportionally.
    pub time_budget: Option<std::time::Duration>,
    /// Per-constraint weights controlling how strongly each soft
    /// constraint contributes to the objective. See [`SolverConfig`]
    /// for details. Default values preserve the historical behaviour
    /// (mostly 1, cox cooldown = 5).
    pub config: SolverConfig,
    /// How many distinct lineups to return. `1` (the default) gives
    /// the historical behaviour — the single best solution lands in
    /// `SolveResult.lineups`. `N > 1` additionally populates
    /// `SolveResult.alternatives` with up to `N - 1` further
    /// lineups, each guaranteed to differ from every previous one
    /// by at least [`SolveRequest::tabu_min_diff`] placements. The
    /// alternatives are ranked best-first by the same objective
    /// function as the primary solution; each successive alternative
    /// is strictly worse than (or tied with) its predecessor.
    ///
    /// If the solver exhausts the feasible region before producing
    /// `N` distinct alternatives (too small a roster, too tight a
    /// tabu radius, too many hard constraints) the result carries
    /// fewer alternatives than requested rather than erroring.
    pub top_n: usize,
    /// Minimum number of per-seat placements that must differ
    /// between any two returned alternatives. Ignored when
    /// `top_n == 1`. A "placement" is a single `(rower, boat,
    /// seat)` triple; swapping two rowers between seats therefore
    /// counts as 2 placement differences, which is also the default
    /// — forcing every alternative to differ from all previous ones
    /// by at least one rower swap.
    ///
    /// Larger values yield more-obviously-distinct alternatives at
    /// the cost of fewer of them being feasible. Setting this to
    /// `0` degenerates to "re-solve the same problem" and returns
    /// exact duplicates (not useful).
    pub tabu_min_diff: i32,
    /// Reference lineups for similarity scoring. Each reference
    /// lineup carries a signed weight:
    ///
    /// - **Positive weight → avoid similarity** (novelty). The solver
    ///   penalises placements that match the reference.
    /// - **Negative weight → prefer similarity** (baseline / carry-
    ///   forward). The solver rewards placements that match.
    ///
    /// Placements whose rower is absent or whose boat isn't in the
    /// candidate fleet are silently dropped. Empty by default.
    ///
    /// Common use cases:
    /// - **Novelty:** the handler builds references from recent
    ///   committed lineups with positive weight, so the solver
    ///   avoids repeating the same seats week after week.
    /// - **No-show re-solve:** the committed lineup for today with
    ///   negative weight + the no-show rower removed from available.
    /// - **Carry-forward:** a previous practice's lineup with
    ///   negative weight, adapted to different attendance.
    pub reference_lineups: Vec<ReferenceLineup>,
    /// Pre-pinned seat assignments. Each lock forces the named rower
    /// into the named seat on the named boat. The solver posts
    /// `x[r, b, s] = 1` and `use[b] = 1` for each lock. Invalid
    /// locks (unknown rower/boat, ineligible seat) are surfaced as
    /// diagnostics and skipped. Empty by default.
    pub locks: Vec<SeatLock>,
    /// Boats the solver MUST field (`use[b] = 1`). Unlike seat locks,
    /// these don't pin any specific rower — they just force the boat
    /// to be used. Used by the boat pin/lock state machine.
    pub required_boats: Vec<BoatId>,
}

/// A set of placements from a committed lineup, scored as a group
/// against the current solution. Positive `weight` penalises
/// similarity (novelty); negative `weight` rewards it (baseline).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceLineup {
    pub placements: Vec<ReferencePlacement>,
    pub weight: i32,
}

/// A single `(rower, boat, seat)` triple within a [`ReferenceLineup`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencePlacement {
    pub rower_id: RowerId,
    pub boat_id: BoatId,
    pub seat: i32,
}

impl SolveRequest {
    /// Whether the request asks for more than one lineup back. Used
    /// by the solver to skip the tabu re-solve plumbing when the
    /// caller wants the historical single-solution behaviour.
    fn wants_alternatives(&self) -> bool {
        self.top_n > 1
    }
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
    // novelty_weight removed — weight is now per-reference-lineup
    // on SolveRequest.reference_lineups.
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
    /// S11 skill-gradient multiplier. Applies a tapering skill
    /// reward across all seats: full weight on end pairs (bow pair
    /// + stern pair zones), with a gradient into the engine room
    /// (3/4, 1/2, 1/4 by distance from the nearest end). This
    /// creates a skill ordering within the engine room — seats 5/6
    /// in an 8+ attract more skilled rowers than 3/4.
    ///
    /// The gradient uses integer arithmetic with a `max(1, …)`
    /// floor, so **weights below 4 produce a flat reward** across
    /// all seats (no gradient). Set to 4+ for the full tapering
    /// effect. Default **1** (flat fallback).
    ///
    /// Most useful as a fallback when rowers don't have explicit
    /// zone preferences (S3). Once a team has zone affinities
    /// dialled in, S3 provides more coach-specific control.
    pub end_pair_skill_weight: i32,
    /// S12 engine-room strength-reward multiplier. Rewards placing
    /// strong rowers in the engine room zone (seats {3,4,5,6} in
    /// an 8+, {2,3} in a 4+, empty in pairs). Same negative-
    /// coefficient encoding as S11 but over `seat_strength` vars.
    /// Default **1**.
    ///
    /// Like S11, most useful as a fallback when rowers don't have
    /// explicit zone preferences (S3).
    pub engine_room_strength_weight: i32,
    /// Per-filled-optional-seat reward under non-strict
    /// [`PartialFillPolicy`]. Each optional seat that ends up
    /// occupied contributes `-partial_fill_bonus` to the
    /// objective, so the solver strictly prefers filling optional
    /// seats when the resulting arrangement is otherwise equal or
    /// close. Inert under `PartialFillPolicy::Strict` because
    /// H1's equality form already forces every seat to be
    /// filled; the bonus is only posted when the partial-fill
    /// policy actually permits empty seats. Default **1** — big
    /// enough to break ties against "leave it empty" but small
    /// enough not to override S1 / S9 / S11 / S12 structural
    /// preferences that would make the extra placement worse.
    pub partial_fill_bonus: i32,
    /// S13 retention reward. Per-rower bonus for placing a rower,
    /// scaled by `abs(sweep_bias) + 1` so hard-locked rowers (±2)
    /// get the strongest retention and ambivalent rowers (0) get
    /// the weakest. All rowers get some retention. Encoded as one
    /// `rower_used[r] ∈ {0, 1}` aux var per eligible rower,
    /// linked to `Σ_{b, s} x[r, b, s]`, with a single scaled obj
    /// term so the total contribution is O(available rowers).
    /// Default **2** — breaks the "who gets benched" tie toward
    /// rowers with strong type preferences without overriding hard
    /// structural preferences (side, skill, weight-class) that
    /// might make placing a specific rower uneconomical.
    pub non_scull_retention_weight: i32,
    /// S14 bow-loader cox fit penalty. Penalises tall and heavy
    /// rowers in the cox seat of bow-loader boats, where the
    /// compartment is tight. Height is the primary factor; weight
    /// is secondary. Penalties are applied per-rower based on
    /// their height and weight ordinals. Default **1** (use the
    /// penalty table as-is).
    pub bow_cox_fit_weight: i32,
    /// S16 top-boat stacking bonus. Extra per-seat skill + strength
    /// reward for the first (largest) boat. The solver concentrates
    /// the best rowers there. Default **0** (off in balanced; tiered
    /// sets it to 2).
    pub top_boat_stacking_weight: i32,
    /// S17 pair-eligibility weight. For pair boats (seat_count=2, no
    /// cox): penalises Intermediate rowers (Master/Expert are free).
    /// Also penalises strength mismatch between the two rowers.
    /// Novices are hard-gated from pairs entirely (H7). Default **3**.
    pub pair_eligibility_weight: i32,
    /// S18 minimize-bench weight. Per-rower reward for being placed
    /// in any seat. Applies to all available rowers (unlike S13 which
    /// scales by sweep_bias). Higher = fewer benched rowers.
    /// Default **4**.
    pub minimize_bench_weight: i32,
    /// S19 boat-size stacking weight. Quality reward inversely scaled
    /// by boat size — strong rowers in smaller boats get a bigger
    /// reward. Used by even_speed to concentrate talent in 4s/pairs
    /// (which need it more than 8s). Default **0** (off; even_speed
    /// sets it to 2).
    pub boat_size_stacking_weight: i32,
    /// S20 bench-cooldown penalty. Penalises benching a rower who
    /// was benched at a recent practice. Linear decay over a window
    /// (like S6 cox cooldown). Default **2**.
    pub bench_cooldown_penalty: i32,
    /// S21 stroke-spread penalty. Penalises placing multiple
    /// "designated strokes" (rowers with SeatZone::Stroke affinity
    /// weight >= 2) in the same boat. For N designated strokes in
    /// one boat, the penalty is `weight * (1 + 2 + ... + (N-1))` =
    /// `weight * N*(N-1)/2`. This spreads stroke talent across boats
    /// without preventing them from being placed. Default **2**.
    pub stroke_spread_weight: i32,
    /// Per-boat-class biases. Scales the S8 placement reward for
    /// boats of each class by `(1 + bias)`. All default **0** (no
    /// preference). Positive = prefer fielding that class.
    pub eight_bias: i32,
    pub coxed_four_bias: i32,
    pub four_bias: i32,
    pub quad_bias: i32,
    pub pair_bias: i32,
    pub double_bias: i32,
    pub single_bias: i32,
}

/// Boat class determined by (seat_count, has_cox, oars_per_seat).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoatClass {
    Eight,     // 8+
    CoxedFour, // 4+
    Four,      // 4-
    Quad,      // 4x
    Pair,      // 2-
    Double,    // 2x
    Single,    // 1x
}

impl BoatClass {
    /// Classify a boat by its physical properties.
    pub fn from_boat(boat: &Boat) -> Self {
        match (
            boat.seat_count.as_int(),
            boat.has_cox.as_bool(),
            boat.oars_per_seat.as_int(),
        ) {
            (8, _, 1) => Self::Eight,
            (4, true, 1) => Self::CoxedFour,
            (4, false, 1) => Self::Four,
            (4, _, 2) => Self::Quad,
            (2, _, 1) => Self::Pair,
            (2, _, 2) => Self::Double,
            (1, _, _) => Self::Single,
            // Fallback: treat anything large as eight-like, small as four-like.
            (n, _, _) if n >= 8 => Self::Eight,
            (n, true, _) if n >= 2 => Self::CoxedFour,
            _ => Self::Four,
        }
    }
}

impl SolverConfig {
    /// Look up the bias for a given boat class.
    pub fn class_bias(&self, class: BoatClass) -> i32 {
        match class {
            BoatClass::Eight => self.eight_bias,
            BoatClass::CoxedFour => self.coxed_four_bias,
            BoatClass::Four => self.four_bias,
            BoatClass::Quad => self.quad_bias,
            BoatClass::Pair => self.pair_bias,
            BoatClass::Double => self.double_bias,
            BoatClass::Single => self.single_bias,
        }
    }
}

impl SolverConfig {
    /// **Balanced** — even-handed defaults. No particular emphasis
    /// on boat speed parity or top-boat stacking.
    pub fn balanced() -> Self {
        Self {
            skill_variance_weight: 1,
            pair_affinity_weight: 4,
            seat_affinity_weight: 5,
            side_preference_weight: 2,
            weight_class_slack_weight: 3,
            cox_cooldown_penalty: 5,
            placement_reward_weight: 4,
            pair_strength_weight: 1,
            bow_pair_strength_weight: 2,
            height_balance_weight: 1,
            end_pair_skill_weight: 1,
            engine_room_strength_weight: 1,
            partial_fill_bonus: 4,
            non_scull_retention_weight: 2,
            bow_cox_fit_weight: 1,
            top_boat_stacking_weight: 0,
            pair_eligibility_weight: 3,
            minimize_bench_weight: 4,
            boat_size_stacking_weight: 0,
            bench_cooldown_penalty: 2,
            stroke_spread_weight: 2,
            eight_bias: 0,
            coxed_four_bias: 0,
            four_bias: 0,
            quad_bias: 0,
            pair_bias: 0,
            double_bias: 0,
            single_bias: 0,
        }
    }

    /// **Even speed** — boats should be as close in speed as possible.
    /// High skill-variance penalty keeps talent spread evenly. High
    /// pair-strength balance means no boat gets all the strong rowers.
    /// Social preferences (pair/seat affinity) dialed back so speed
    /// parity wins. Side preference gentle but respected.
    pub fn even_speed() -> Self {
        Self {
            skill_variance_weight: 3,
            pair_affinity_weight: 1,   // speed parity > being with friends
            seat_affinity_weight: 2,   // nice-to-have, not priority
            side_preference_weight: 2, // respect side but allow overrides
            pair_strength_weight: 3,
            bow_pair_strength_weight: 3,
            height_balance_weight: 2,
            end_pair_skill_weight: 0,
            engine_room_strength_weight: 0,
            top_boat_stacking_weight: -2, // penalize stacking = spread talent
            boat_size_stacking_weight: 3, // stack smaller boats for speed parity
            ..Self::balanced()
        }
    }

    /// **Tiered / coached** — top boat stacked with the best rowers.
    /// Low skill-variance penalty lets talent concentrate. High
    /// end-pair skill and engine-room strength rewards place the
    /// strongest rowers in the most impactful seats.
    /// **Tiered / coached** — top boat stacked with the best rowers.
    /// Skill gaps between boats are expected. Seat preferences yield
    /// to stacking but pair affinity stays high (coach set those
    /// pairs for a reason).
    pub fn tiered() -> Self {
        Self {
            skill_variance_weight: 0,
            seat_affinity_weight: 2, // yield to stacking, not ignored
            end_pair_skill_weight: 1,
            engine_room_strength_weight: 1,
            pair_strength_weight: 0,
            bow_pair_strength_weight: 1,
            top_boat_stacking_weight: 4,
            minimize_bench_weight: 2, // strategic benching OK, not aggressive
            bench_cooldown_penalty: 1, // rotate who gets benched, don't repeat
            ..Self::balanced()
        }
    }

    /// **Random** — talent shuffled freely for maximum variety.
    /// Quality and social constraints off, but safety-critical floors
    /// kept: side preference, weight class, pair eligibility, bow cox
    /// fit. Everyone still rows (minimize bench + placement reward).
    pub fn random() -> Self {
        Self {
            skill_variance_weight: 0,
            pair_affinity_weight: 0,
            seat_affinity_weight: 0,
            side_preference_weight: 2,    // wrong-side is hard to row
            weight_class_slack_weight: 1, // sensible boat sizes
            cox_cooldown_penalty: 0,
            placement_reward_weight: 2, // field boats, don't leave them idle
            pair_strength_weight: 0,
            bow_pair_strength_weight: 0,
            height_balance_weight: 0,
            end_pair_skill_weight: 0,
            engine_room_strength_weight: 0,
            partial_fill_bonus: 1, // partial-fill still prefers filling
            non_scull_retention_weight: 0,
            bow_cox_fit_weight: 1, // don't stuff heavyweights in bow cox
            top_boat_stacking_weight: 0,
            pair_eligibility_weight: 1, // floor for pair boat safety
            minimize_bench_weight: 2,   // everyone rows
            boat_size_stacking_weight: 0,
            bench_cooldown_penalty: 0,
            stroke_spread_weight: 0,
            eight_bias: 0,
            coxed_four_bias: 0,
            four_bias: 0,
            quad_bias: 0,
            pair_bias: 0,
            double_bias: 0,
            single_bias: 0,
        }
    }

    /// Built-in preset names. Custom profiles must not shadow these.
    pub const BUILTIN_NAMES: &'static [&'static str] =
        &["balanced", "even_speed", "tiered", "random"];

    /// Whether a name is a reserved built-in preset.
    pub fn is_builtin(name: &str) -> bool {
        Self::BUILTIN_NAMES.contains(&name)
    }

    /// Look up a preset by name. Returns `None` for unknown names.
    pub fn from_preset(name: &str) -> Option<Self> {
        match name {
            "balanced" => Some(Self::balanced()),
            "even_speed" => Some(Self::even_speed()),
            "tiered" => Some(Self::tiered()),
            "random" => Some(Self::random()),
            _ => None,
        }
    }
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self::balanced()
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
    /// Solver outcome for the *primary* (best) solution. When
    /// `status != Satisfied`, `primary` holds an empty
    /// `ProposedSolution` and `alternatives` is empty — an
    /// unsatisfiable or timed-out problem yields no lineups at
    /// all.
    pub status: SolveStatus,
    /// The best solution the solver found. Holds both the
    /// per-boat lineup assignments and the bucketed breakdown of
    /// available rowers who didn't make it in. Callers that
    /// don't care about Top-N alternatives can stop here.
    pub primary: ProposedSolution,
    /// Additional distinct solutions, ranked best-first, each
    /// guaranteed to differ from every preceding one (including
    /// `primary`) by at least [`SolveRequest::tabu_min_diff`]
    /// placements. Empty when [`SolveRequest::top_n`] is 1 (the
    /// default) or when the solver couldn't find any further
    /// distinct feasible assignments under the tabu radius.
    /// Each entry carries its own `lineups` and `unplaced` so a
    /// coach comparing alternatives can see both "who rows" and
    /// "who sits out" per alternative without having to
    /// recompute.
    pub alternatives: Vec<ProposedSolution>,
    /// Pre-solve diagnostics explaining why the problem is (or
    /// is likely) unsatisfiable. Empty when `status == Satisfied`.
    /// Populated by cheap eligibility checks before the solver
    /// runs, so the coach gets actionable feedback instantly.
    pub diagnostics: Vec<Diagnostic>,
    /// Wall-clock time the solver spent (model build + search).
    pub elapsed: std::time::Duration,
    /// Best objective value found (lower is better). None when
    /// unsatisfiable or unknown.
    pub objective: Option<i32>,
}

/// A single self-contained proposal from the solver: one
/// [`ProposedLineup`] per candidate boat *plus* the bucketed
/// breakdown of available rowers who aren't in it. Used for
/// both the primary solution and every Top-N alternative so the
/// same shape works everywhere a coach would display, compare,
/// or commit a lineup set.
///
/// Kept flat (rather than mashing everything onto `SolveResult`)
/// because a coach's mental model is "show me options", where
/// each option is (here's who rows + here's who sits out).
/// Alternatives inherit the exact same shape as the primary,
/// making iteration over all N solutions symmetric.
#[derive(Debug, Clone, Default)]
pub struct ProposedSolution {
    /// One entry per candidate boat, with `used = true` for the
    /// boats the solver chose to field in this solution. Boats
    /// with `used = false` have empty `seats` — they were
    /// candidates the solver rejected.
    pub lineups: Vec<ProposedLineup>,
    /// Available rowers who didn't make it into `lineups`.
    /// See [`UnplacedRowers`].
    pub unplaced: UnplacedRowers,
}

/// Rowers who were available for seating today but didn't
/// land in a given lineup set. Split by `sweep_bias` so the
/// coach can see at a glance who wasn't placed. Since the solver
/// now handles both sweep and scull boats, all unplaced rowers
/// are simply "benched". The list preserves the stable iteration
/// order of `DbSnapshot.available_rowers()`.
#[derive(Debug, Clone, Default)]
pub struct UnplacedRowers {
    /// Rowers who weren't placed in any boat (sweep or scull).
    pub benched: Vec<RowerId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveStatus {
    Satisfied,
    Unsatisfiable,
    Timeout,
}

/// Event emitted by [`solve_streaming`] as each result becomes available.
#[derive(Debug, Clone)]
pub enum SolveStreamEvent {
    /// Primary solve completed successfully.
    Primary {
        status: SolveStatus,
        solution: ProposedSolution,
        diagnostics: Vec<Diagnostic>,
        objective: Option<i32>,
    },
    /// Primary solve failed (Unsatisfiable or Timeout with no solution).
    PrimaryFailed {
        status: SolveStatus,
        diagnostics: Vec<Diagnostic>,
    },
    /// One alternative completed.
    Alternative {
        index: usize,
        solution: ProposedSolution,
    },
    /// All alternatives done (or none requested).
    Done { elapsed: std::time::Duration },
}

/// A pre-solve diagnostic explaining why a problem is (or is likely)
/// unsatisfiable. Populated by cheap eligibility checks *before*
/// invoking Pumpkin, so the coach gets actionable feedback without
/// waiting for a full solve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnostic {
    /// A coxed boat has no eligible cox among the available rowers.
    NoCoxForBoat { boat_name: String },
    /// Every candidate boat needs more total seats than rowers
    /// available. Even the smallest boat can't be fully crewed.
    NotEnoughRowers {
        available: usize,
        smallest_boat_seats: usize,
        smallest_boat_name: String,
    },
    /// A specific required seat on a boat has zero eligible rowers
    /// (after side + cox + designated-cox filtering).
    UnfillableSeat { boat_name: String, seat: i32 },
    /// Every candidate boat was forced unused because at least one
    /// of its required seats is unfillable — no fleet can be fielded.
    AllBoatsUnfillable,
    /// A seat lock refers to a rower/boat/seat combination that
    /// doesn't exist or isn't eligible. The lock is skipped.
    InvalidLock {
        rower_name: String,
        boat_name: String,
        seat: i32,
        reason: String,
    },
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

/// Find a feasible seat assignment for the requested boats with
/// soft-constraint-aware optimisation and optional Top-N
/// alternatives.
///
/// The body is a thin three-phase orchestrator:
///
/// 1. **Resolve inputs.** Turn `request.boats` into a concrete
///    `Vec<&Boat>` (empty = "every in-service boat"),
///    gather the available rowers for the date, and short-circuit
///    the trivial cases where there's nothing to do.
/// 2. **Build the model.** Delegate to [`build_model`] to
///    assemble a fully-constrained `ModelBuilder` and linked
///    objective variable.
/// 3. **Search for lineups.** Delegate to [`search_lineups`] to
///    run the Pumpkin optimiser (with Top-N tabu re-solve if
///    requested) and decode the result.
///
/// Each phase is self-contained; the split makes it obvious
/// where "assembling constraints" ends and "running search"
/// begins.
#[tracing::instrument(level = "debug", skip_all, fields(date = %request.date, n_boats = request.boats.len()), err)]
pub fn solve(snapshot: &DbSnapshot, request: &SolveRequest) -> Result<SolveResult> {
    // --- Phase 1: resolve inputs ---
    //
    // Turn `request.boats` into a concrete fleet vec. An empty
    // request means "consider every in-service boat"; any
    // explicit IDs must correspond to actual snapshot entries.
    let boats: Vec<&Boat> = if request.boats.is_empty() {
        snapshot.boats.iter().collect()
    } else {
        request
            .boats
            .iter()
            .map(|bid| {
                snapshot
                    .boats
                    .iter()
                    .find(|b| b.id == *bid)
                    .ok_or_else(|| anyhow!("boat {} not in snapshot fleet", bid))
            })
            .collect::<Result<_>>()?
    };
    let solve_start = std::time::Instant::now();

    if boats.is_empty() {
        return Ok(SolveResult {
            status: SolveStatus::Satisfied,
            primary: ProposedSolution::default(),
            alternatives: vec![],
            diagnostics: vec![],
            elapsed: solve_start.elapsed(),
            objective: None,
        });
    }

    let available: Vec<&Rower> = snapshot.available_rowers().collect();
    if available.is_empty() {
        bail!("no rowers are available for seating on {}", request.date);
    }

    // --- Phase 1b: greedy fleet pre-selection ---
    //
    // The CP solver explores the search space incrementally and may
    // time out before discovering that two large boats are better
    // than one large + one small. Help it by pre-selecting a greedy
    // fleet: sort boats by size descending and pick boats until
    // capacity is exhausted. This doesn't prevent the solver from
    // choosing differently — it just prunes obviously-suboptimal
    // candidates so the solver converges faster.
    let mut boats = greedy_fleet_select(boats, &available, request.partial_fill);

    // Ensure required boats (from pins/locks) survive greedy selection.
    for &req_bid in &request.required_boats {
        if !boats.iter().any(|b| b.id == req_bid) {
            if let Some(b) = snapshot.boats.iter().find(|b| b.id == req_bid) {
                boats.push(b);
            }
        }
    }
    // Also ensure boats with seat locks survive greedy selection.
    for lock in &request.locks {
        if !boats.iter().any(|b| b.id == lock.boat_id) {
            if let Some(b) = snapshot.boats.iter().find(|b| b.id == lock.boat_id) {
                boats.push(b);
            }
        }
    }

    // --- Phase 1c: pre-solve diagnostics ---
    let mut diagnostics = pre_solve_diagnostics(&boats, &available);

    // --- Phase 2: build the model ---
    let rss_before = rss_kb();
    let (builder, objective) = build_model(snapshot, request, boats, available, &mut diagnostics)?;

    // --- Phase 3: search ---
    let mut result = search_lineups(builder, objective, request, diagnostics)?;
    result.elapsed = solve_start.elapsed();

    let rss_after = rss_kb();
    tracing::info!(
        rss_before_mib = format!("{:.2}", rss_before as f64 / 1024.0),
        rss_after_mib = format!("{:.2}", rss_after as f64 / 1024.0),
        rss_delta_mib = format!(
            "{:.2}",
            rss_after.saturating_sub(rss_before) as f64 / 1024.0
        ),
        elapsed_ms = result.elapsed.as_millis() as u64,
        "solve complete"
    );

    Ok(result)
}

/// Streaming solver: sends primary result and each alternative through
/// a channel as they complete, rather than accumulating and returning
/// all at once. The caller (typically the SSE handler) converts the
/// channel into an event stream.
///
/// Uses `std::sync::mpsc::SyncSender` (blocking) because this runs on
/// the rayon pool — not an async runtime.
#[tracing::instrument(level = "debug", skip_all, fields(date = %request.date), err)]
pub fn solve_streaming(
    snapshot: &DbSnapshot,
    request: &SolveRequest,
    tx: std::sync::mpsc::SyncSender<SolveStreamEvent>,
) -> Result<()> {
    let boats: Vec<&Boat> = if request.boats.is_empty() {
        snapshot.boats.iter().collect()
    } else {
        request
            .boats
            .iter()
            .map(|bid| {
                snapshot
                    .boats
                    .iter()
                    .find(|b| b.id == *bid)
                    .ok_or_else(|| anyhow!("boat {} not in snapshot fleet", bid))
            })
            .collect::<Result<_>>()?
    };
    let solve_start = std::time::Instant::now();

    if boats.is_empty() {
        let _ = tx.send(SolveStreamEvent::Primary {
            status: SolveStatus::Satisfied,
            solution: ProposedSolution::default(),
            diagnostics: vec![],
            objective: None,
        });
        let _ = tx.send(SolveStreamEvent::Done {
            elapsed: solve_start.elapsed(),
        });
        return Ok(());
    }

    let available: Vec<&Rower> = snapshot.available_rowers().collect();
    if available.is_empty() {
        bail!("no rowers are available for seating on {}", request.date);
    }

    let mut boats = greedy_fleet_select(boats, &available, request.partial_fill);
    for &req_bid in &request.required_boats {
        if !boats.iter().any(|b| b.id == req_bid) {
            if let Some(b) = snapshot.boats.iter().find(|b| b.id == req_bid) {
                boats.push(b);
            }
        }
    }
    for lock in &request.locks {
        if !boats.iter().any(|b| b.id == lock.boat_id) {
            if let Some(b) = snapshot.boats.iter().find(|b| b.id == lock.boat_id) {
                boats.push(b);
            }
        }
    }

    let mut diagnostics = pre_solve_diagnostics(&boats, &available);
    let (mut builder, objective) =
        build_model(snapshot, request, boats, available, &mut diagnostics)?;

    let mut brancher = builder.solver.default_brancher();
    let mut resolver = ResolutionResolver::default();

    let run_one =
        |solver: &mut Solver, brancher: &mut _, resolver: &mut _| -> OptimisationResult<()> {
            match request.time_budget {
                None => {
                    let mut termination = Indefinite;
                    let procedure =
                        LinearSatUnsat::new(OptimisationDirection::Minimise, objective, NoCallback);
                    solver.optimise(brancher, &mut termination, resolver, procedure)
                }
                Some(budget) => {
                    let mut termination = TimeBudget::starting_now(budget);
                    let procedure =
                        LinearSatUnsat::new(OptimisationDirection::Minimise, objective, NoCallback);
                    solver.optimise(brancher, &mut termination, resolver, procedure)
                }
            }
        };

    // Primary solve
    let primary_opt = run_one(&mut builder.solver, &mut brancher, &mut resolver);
    let (_primary_lineups, primary_placements) = match primary_opt {
        OptimisationResult::Optimal(sol)
        | OptimisationResult::Satisfiable(sol)
        | OptimisationResult::Stopped(sol, _) => {
            let obj_val = sol.get_integer_value(objective);
            let lineups = decode_solution(
                &builder.x,
                &builder.use_b,
                &builder.boats,
                &builder.available,
                |v| sol.get_integer_value(v),
            );
            let placements = collect_placements(&builder.x, |v| sol.get_integer_value(v));
            let unplaced = compute_unplaced(&builder.available, &lineups);
            let _ = tx.send(SolveStreamEvent::Primary {
                status: SolveStatus::Satisfied,
                solution: ProposedSolution {
                    lineups: lineups.clone(),
                    unplaced,
                },
                diagnostics: diagnostics.clone(),
                objective: Some(obj_val),
            });
            (lineups, placements)
        }
        OptimisationResult::Unsatisfiable => {
            let _ = tx.send(SolveStreamEvent::PrimaryFailed {
                status: SolveStatus::Unsatisfiable,
                diagnostics,
            });
            let _ = tx.send(SolveStreamEvent::Done {
                elapsed: solve_start.elapsed(),
            });
            return Ok(());
        }
        OptimisationResult::Unknown => {
            let _ = tx.send(SolveStreamEvent::PrimaryFailed {
                status: SolveStatus::Timeout,
                diagnostics: vec![],
            });
            let _ = tx.send(SolveStreamEvent::Done {
                elapsed: solve_start.elapsed(),
            });
            return Ok(());
        }
    };

    // Alternatives
    if request.wants_alternatives() {
        if post_tabu_constraint(
            &mut builder.solver,
            &primary_placements,
            request.tabu_min_diff,
        )? {
            let mut alt_index = 0usize;
            for _ in 1..request.top_n {
                // If the receiver is dropped (client disconnected), stop early.
                let next = run_one(&mut builder.solver, &mut brancher, &mut resolver);
                let sol = match next {
                    OptimisationResult::Optimal(sol)
                    | OptimisationResult::Satisfiable(sol)
                    | OptimisationResult::Stopped(sol, _) => sol,
                    OptimisationResult::Unsatisfiable | OptimisationResult::Unknown => break,
                };

                let alt_lineups = decode_solution(
                    &builder.x,
                    &builder.use_b,
                    &builder.boats,
                    &builder.available,
                    |v| sol.get_integer_value(v),
                );
                let alt_placements = collect_placements(&builder.x, |v| sol.get_integer_value(v));
                let alt_unplaced = compute_unplaced(&builder.available, &alt_lineups);

                if tx
                    .send(SolveStreamEvent::Alternative {
                        index: alt_index,
                        solution: ProposedSolution {
                            lineups: alt_lineups,
                            unplaced: alt_unplaced,
                        },
                    })
                    .is_err()
                {
                    // Receiver dropped — client disconnected.
                    return Ok(());
                }

                alt_index += 1;
                if alt_index + 1 < request.top_n
                    && !post_tabu_constraint(
                        &mut builder.solver,
                        &alt_placements,
                        request.tabu_min_diff,
                    )?
                {
                    break;
                }
            }
        }
    }

    let _ = tx.send(SolveStreamEvent::Done {
        elapsed: solve_start.elapsed(),
    });
    Ok(())
}

/// Read the process RSS in KB from /proc/self/statm (Linux only).
/// Returns 0 on non-Linux or if the read fails.
fn rss_kb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
            .map(|pages| pages * 4) // pages are 4KB on x86/arm
            .unwrap_or(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// Greedy fleet pre-selection: pick the largest boats first until
/// the available rower count is exhausted. Returns a subset of
/// `candidates` that can plausibly be fully (or partially) filled.
///
/// This is a heuristic — the solver still decides the final fleet
/// via `use[b]` variables. But narrowing the candidate set from
/// "all in-service boats" to "the ones that actually fit"
/// dramatically reduces the search space and prevents timeout on
/// fleet-configuration exploration.
fn greedy_fleet_select<'a>(
    mut candidates: Vec<&'a Boat>,
    available: &[&Rower],
    partial_fill: PartialFillPolicy,
) -> Vec<&'a Boat> {
    use model::{boat_target_weight_ordinal, optional_seats};

    let num_available = available.len();

    // Compute the average weight class of the strongest rowers.
    // "Strongest" = highest skill + strength ordinal sum. We look
    // at the top N where N = largest boat's rowing seat count, so
    // the weight-class match is for the rowers who'd fill the top
    // boat.
    let top_n = candidates
        .iter()
        .map(|b| b.seat_count.as_int() as usize)
        .max()
        .unwrap_or(8);
    // Quality heuristic: skill + strength + power-to-weight bonus.
    // A rower whose strength exceeds their weight class is more
    // effective per kg. This prevents heavy-but-weak rowers from
    // pulling the top boat toward a heavier weight class.
    let mut rower_quality: Vec<(i32, i32)> = available
        .iter()
        .filter(|r| !r.is_designated_cox.as_bool())
        .map(|r| {
            let pw_bonus = (r.strength.ordinal() - r.weight_class.ordinal()).max(0);
            let quality = r.skill.ordinal() + r.strength.ordinal() + pw_bonus;
            (quality, r.weight_class.ordinal())
        })
        .collect();
    rower_quality.sort_by(|a, b| b.0.cmp(&a.0)); // best first
                                                 // Quality-weighted average: strong rowers' weight class counts
                                                 // more than weak rowers'. This reflects power-to-weight: a heavy
                                                 // Expert matters more for boat selection than a heavy Novice.
    let top_avg_weight: f64 = if rower_quality.is_empty() {
        2.0
    } else {
        let n = top_n.min(rower_quality.len());
        let top = &rower_quality[..n];
        let total_quality: f64 = top.iter().map(|(q, _)| *q as f64).sum();
        if total_quality > 0.0 {
            top.iter().map(|(q, w)| *q as f64 * *w as f64).sum::<f64>() / total_quality
        } else {
            top.iter().map(|(_, w)| *w as f64).sum::<f64>() / n as f64
        }
    };

    // Sort: largest boats first. Among same-sized boats, count how
    // many strong heavy rowers (quality above median, weight >= Heavy)
    // are available. If there are enough to justify a heavy boat,
    // put it first. Otherwise fall back to heavier-boat-first as a
    // tiebreaker (it's more forgiving of mixed-weight crews).
    let heavy_strong_count = rower_quality
        .iter()
        .filter(|(q, w)| *w >= 3 && *q >= 6) // Heavy+ and decent quality
        .count();

    candidates.sort_by(|a, b| {
        let a_total = a.seat_count.as_int() + if a.has_cox.as_bool() { 1 } else { 0 };
        let b_total = b.seat_count.as_int() + if b.has_cox.as_bool() { 1 } else { 0 };
        b_total.cmp(&a_total).then_with(|| {
            if heavy_strong_count >= 2 {
                // Enough strong heavies — put the heavier boat first
                // so they don't fight the weight-class wall.
                let a_wc = boat_target_weight_ordinal(a.weight_class);
                let b_wc = boat_target_weight_ordinal(b.weight_class);
                b_wc.cmp(&a_wc)
            } else {
                // Few strong heavies — put the lighter boat first
                // since the top rowers are mostly lighter.
                let a_wc = boat_target_weight_ordinal(a.weight_class);
                let b_wc = boat_target_weight_ordinal(b.weight_class);
                a_wc.cmp(&b_wc)
            }
        })
    });

    let k = partial_fill.max_empty();
    let mut remaining = num_available as i32;
    let mut selected = Vec::new();

    for boat in &candidates {
        let seats_total = boat.seat_count.as_int() + if boat.has_cox.as_bool() { 1 } else { 0 };
        let n_opt = optional_seats(boat).len() as i32;
        let can_skip = k.min(n_opt);
        let min_seats = seats_total - can_skip;

        if remaining >= min_seats {
            selected.push(*boat);
            tracing::debug!(
                boat = %boat.name,
                seats_total,
                min_seats,
                remaining_before = remaining,
                "greedy: selected boat"
            );
            // Reserve only min_seats worth of rowers so subsequent
            // boats can still qualify. The solver decides the actual
            // fill level via use[b] + partial-fill constraints.
            remaining -= min_seats;
        } else {
            tracing::debug!(
                boat = %boat.name,
                seats_total,
                min_seats,
                remaining,
                "greedy: skipped boat (insufficient rowers)"
            );
        }
    }

    tracing::info!(
        candidates = candidates.len(),
        selected = selected.len(),
        top_avg_weight = format!("{:.1}", top_avg_weight),
        names = %selected.iter().map(|b| format!("{} ({})", b.name, b.weight_class)).collect::<Vec<_>>().join(", "),
        "greedy fleet pre-selection"
    );

    selected
}

/// Cheap pre-solve checks that detect common reasons for
/// unsatisfiability before Pumpkin is invoked. Returns an empty
/// vec when everything looks feasible.
fn pre_solve_diagnostics(boats: &[&Boat], available: &[&Rower]) -> Vec<Diagnostic> {
    use model::{rower_eligible_for_seat, seat_positions};

    let mut diags = Vec::new();

    // Track which boats have at least one unfillable required seat.
    let mut all_boats_unfillable = true;

    for boat in boats {
        let mut boat_unfillable = false;
        let positions = seat_positions(boat);

        // Check each seat for at least one eligible rower.
        for &seat in &positions {
            let eligible_count = available
                .iter()
                .filter(|r| rower_eligible_for_seat(r, boat, seat))
                .count();
            if eligible_count == 0 {
                if seat == 0 {
                    diags.push(Diagnostic::NoCoxForBoat {
                        boat_name: boat.name.clone(),
                    });
                } else {
                    diags.push(Diagnostic::UnfillableSeat {
                        boat_name: boat.name.clone(),
                        seat,
                    });
                }
                boat_unfillable = true;
            }
        }

        if !boat_unfillable {
            all_boats_unfillable = false;
        }
    }

    if all_boats_unfillable && !boats.is_empty() {
        diags.push(Diagnostic::AllBoatsUnfillable);
    }

    // Check if even the smallest boat can't be crewed.
    let smallest = boats
        .iter()
        .map(|b| {
            let total = b.seat_count.as_int() as usize + if b.has_cox.as_bool() { 1 } else { 0 };
            (total, &b.name)
        })
        .min_by_key(|(seats, _)| *seats);
    if let Some((smallest_seats, smallest_name)) = smallest {
        if available.len() < smallest_seats {
            diags.push(Diagnostic::NotEnoughRowers {
                available: available.len(),
                smallest_boat_seats: smallest_seats,
                smallest_boat_name: smallest_name.clone(),
            });
        }
    }

    diags
}

/// Assemble the Pumpkin model for a single solve request.
///
/// Takes the pre-resolved candidate fleet and availability list,
/// spins up a fresh [`ModelBuilder`], and posts every active
/// constraint in the order the solver expects:
///
/// 1. S8 placement reward (per-boat objective term, needs
///    `use_b` but not `x`, so it runs before variable creation)
/// 2. Variable creation — populates `x` and
///    `wrong_side_by_rower` for the fleet-level softs
/// 3. Fleet-level soft constraints (S4, S6, S7) that aggregate
///    across rowers / historical lineups
/// 4. Hard constraints (H1, H2, H6, H5 + the S5 slack bundled
///    with it) plus the partial-fill bonus
/// 5. Shared per-seat trait maps (`seat_skill_by_seat`,
///    `seat_strength_by_seat`, `seat_height_by_seat`) for the
///    seat-level softs that consume them
/// 6. Seat-level soft constraints (S1, S2, S3, S9/S9b, S10,
///    S11, S12)
/// 7. Objective variable — allocated in `[-10_000, 10_000]` and
///    linked to the accumulated `obj_terms` via a single linear
///    equality.
///
/// Returns the fully-assembled builder plus the `objective`
/// `DomainId` so the search phase can hand them to Pumpkin.
/// Precondition: `boats` and `available` are both non-empty
/// (the trivial cases are rejected by `solve` before calling).
fn build_model<'a>(
    snapshot: &'a DbSnapshot,
    request: &SolveRequest,
    boats: Vec<&'a Boat>,
    available: Vec<&'a Rower>,
    lock_diags: &mut Vec<Diagnostic>,
) -> Result<(ModelBuilder<'a>, DomainId)> {
    // All mutable solver state lives on a single `ModelBuilder`.
    // Its methods post whole constraint blocks (variable
    // creation, H1, H2, H5/S5, H6, etc.). Per-constraint weights,
    // the candidate fleet, and the available-rowers list all
    // move onto the builder.
    let mut m = ModelBuilder::new(boats, available, request.config);

    // S8 — per-boat placement reward (`-seats_total · use[b]`).
    // Posted up front so the fleet-selection objective is visible
    // to later constraint propagation, though order only affects
    // reporting — the objective sum is commutative.
    m.post_s8_placement_reward();

    // Create one x[(r, b, s)] ∈ {0, 1} per eligible triple and,
    // along the way, collect wrong-side candidates into
    // `m.wrong_side_by_rower` for S4 to consume. See
    // `ModelBuilder::create_variables` for the full eligibility
    // rules and per-rower aggregation rationale.
    m.create_variables();

    // Seat locks — force specific (rower, boat, seat) placements.
    // Must run after create_variables so x vars exist, but before
    // other constraints so the locks are visible to propagation.
    lock_diags.extend(m.post_seat_locks(&request.locks)?);

    // Required boats — force use[b] = 1 for boats the coach pinned/locked.
    for &required_bid in &request.required_boats {
        if let Some(b_idx) = m.boats.iter().position(|b| b.id == required_bid) {
            let tag = m.solver.new_constraint_tag();
            m.solver
                .add_constraint(pumpkin_constraints::equals(
                    vec![m.use_b[b_idx].scaled(1)],
                    1,
                    tag,
                ))
                .post()
                .map_err(|e| anyhow!("required boat {required_bid}: {e:?}"))?;
        }
    }

    // Fleet-level soft constraints: S4 wrong-side aggregation,
    // S6 cox cooldown, S7 novelty vs recent lineups, S13
    // retention. These live in `soft_fleet.rs` as
    // ModelBuilder methods. S8 already ran above because it
    // only needs `use_b` / `boats` / `cfg` and doesn't touch
    // `x`.
    m.post_s4_wrong_side()?;
    m.post_s6_cox_cooldown(snapshot, request.date)?;
    m.post_s13_non_scull_retention()?;
    m.post_sweep_bias_penalty();
    m.post_s18_minimize_bench()?;
    m.post_s20_bench_cooldown(snapshot, request.date)?;
    m.post_s14_bow_cox_fit()?;
    m.post_s15_designated_cox_retention()?;
    m.post_s21_stroke_spread(snapshot)?;
    m.post_reference_similarity(&request.reference_lineups)?;

    // Hard constraints: H1 seat fill + partial-fill cap, H2
    // rower-at-most-one, H6 fleet capacity, H5 weight-class wall
    // (plus the S5 weight-class slack bundled with it because
    // the wall and the slack iterate the same boat loop and
    // share the `positive_terms` sum). See the method docs on
    // ModelBuilder for the per-constraint rationale.
    m.post_h1_seat_fill(request.partial_fill)?;
    m.post_h2_at_most_one()?;
    m.post_h6_fleet_capacity(request.partial_fill)?;
    m.post_h5_s5_weight_class()?;
    // Partial-fill bonus rewards each occupied optional seat
    // under non-strict partial-fill policies. Inert (no-op) under
    // Strict, so this is safe to call unconditionally.
    m.post_partial_fill_bonus(request.partial_fill)?;

    // Seat-level soft constraints. First build the shared
    // `seat_{skill,strength,height}_by_seat` maps for any trait
    // whose consumer constraints are enabled, then post the
    // per-boat / per-partition / per-end-pair soft terms that
    // read them. All of this lives in `soft_seats.rs`.
    m.build_seat_trait_maps(request.partial_fill)?;
    m.post_s1_skill_variance()?;
    m.post_s2_pair_affinities(snapshot)?;
    m.post_s3_seat_affinities(snapshot)?;
    m.post_s9_pair_strength()?;
    m.post_s10_pair_height()?;
    m.post_s11_end_pair_skill();
    m.post_s12_engine_room_strength();
    m.post_s16_top_boat_stacking();
    m.post_s17_pair_eligibility()?;
    m.post_s19_boat_size_stacking();

    // --- Objective variable ---
    //
    // Sum of every weighted term pushed into `m.obj_terms` by
    // the soft constraint blocks above. A generous domain is
    // fine — Pumpkin propagates tighter bounds from the term
    // domains during search. The lower bound must be negative
    // because S2 pair-affinity / S3 seat-affinity / S11 / S12
    // rewards contribute negative terms.
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

    tracing::info!(
        x_vars = m.x.len(),
        boats = m.boats.len(),
        available = m.available.len(),
        obj_terms = m.obj_terms.len(),
        "solver model built"
    );

    Ok((m, objective))
}

/// Run the Pumpkin optimiser on a built model and decode the
/// result, including the Top-N tabu re-solve loop when the
/// caller asked for alternatives.
///
/// For `top_n == 1` (the default) the loop runs exactly once
/// and this function behaves identically to the pre-Top-N code
/// path. For `top_n > 1`, after each successful solve the
/// routine collects the winning placements as a set of x = 1
/// variables and posts a new linear constraint forbidding a
/// future solution from re-using more than `placements -
/// tabu_min_diff` of them. Pumpkin supports posting constraints
/// between `optimise()` calls — the Solver isn't consumed — so
/// we just rebuild the termination + procedure each iteration
/// and keep accumulating tabu constraints until we have enough
/// alternatives or the feasible region is exhausted.
///
/// Consumes the `ModelBuilder` by value because the search
/// phase mutably borrows solver state across multiple `optimise`
/// calls and there's no good reason to hand the builder back to
/// the caller afterward — once the search is done, the model
/// is spent.
fn search_lineups(
    mut builder: ModelBuilder<'_>,
    objective: DomainId,
    request: &SolveRequest,
    diagnostics: Vec<Diagnostic>,
) -> Result<SolveResult> {
    let mut brancher = builder.solver.default_brancher();
    let mut resolver = ResolutionResolver::default();

    // Helper: run a single optimisation call with the current
    // time-budget policy. Returns the raw Pumpkin result so the
    // caller can decode and decide whether to continue.
    let run_one =
        |solver: &mut Solver, brancher: &mut _, resolver: &mut _| -> OptimisationResult<()> {
            match request.time_budget {
                None => {
                    let mut termination = Indefinite;
                    let procedure =
                        LinearSatUnsat::new(OptimisationDirection::Minimise, objective, NoCallback);
                    solver.optimise(brancher, &mut termination, resolver, procedure)
                }
                Some(budget) => {
                    let mut termination = TimeBudget::starting_now(budget);
                    let procedure =
                        LinearSatUnsat::new(OptimisationDirection::Minimise, objective, NoCallback);
                    solver.optimise(brancher, &mut termination, resolver, procedure)
                }
            }
        };

    tracing::info!(
        boats = builder.boats.len(),
        rowers = builder.available.len(),
        x_vars = builder.x.len(),
        obj_terms = builder.obj_terms.len(),
        "starting primary solve"
    );

    // Primary solve — determines the overall result status.
    let primary_opt = run_one(&mut builder.solver, &mut brancher, &mut resolver);
    let (primary_status, primary_lineups, primary_placements, primary_objective) = match primary_opt
    {
        OptimisationResult::Optimal(sol) => {
            let obj_val = sol.get_integer_value(objective);
            let boat_usage: Vec<_> = builder
                .boats
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let used = sol.get_integer_value(builder.use_b[i]);
                    format!("{}={}", b.name, used)
                })
                .collect();
            tracing::info!(
                objective = obj_val,
                boats = %boat_usage.join(", "),
                "primary solve OPTIMAL"
            );
            let lineups = decode_solution(
                &builder.x,
                &builder.use_b,
                &builder.boats,
                &builder.available,
                |v| sol.get_integer_value(v),
            );
            let placements = collect_placements(&builder.x, |v| sol.get_integer_value(v));
            (SolveStatus::Satisfied, lineups, placements, Some(obj_val))
        }
        OptimisationResult::Satisfiable(sol) => {
            let obj_val = sol.get_integer_value(objective);
            let boat_usage: Vec<_> = builder
                .boats
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let used = sol.get_integer_value(builder.use_b[i]);
                    format!("{}={}", b.name, used)
                })
                .collect();
            tracing::info!(
                objective = obj_val,
                boats = %boat_usage.join(", "),
                "primary solve SATISFIABLE (not proven optimal)"
            );
            let lineups = decode_solution(
                &builder.x,
                &builder.use_b,
                &builder.boats,
                &builder.available,
                |v| sol.get_integer_value(v),
            );
            let placements = collect_placements(&builder.x, |v| sol.get_integer_value(v));
            (SolveStatus::Satisfied, lineups, placements, Some(obj_val))
        }
        OptimisationResult::Stopped(sol, _) => {
            let obj_val = sol.get_integer_value(objective);
            let boat_usage: Vec<_> = builder
                .boats
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let used = sol.get_integer_value(builder.use_b[i]);
                    format!("{}={}", b.name, used)
                })
                .collect();
            tracing::warn!(
                objective = obj_val,
                boats = %boat_usage.join(", "),
                "primary solve TIMED OUT — returning best-so-far"
            );
            let lineups = decode_solution(
                &builder.x,
                &builder.use_b,
                &builder.boats,
                &builder.available,
                |v| sol.get_integer_value(v),
            );
            let placements = collect_placements(&builder.x, |v| sol.get_integer_value(v));
            (SolveStatus::Satisfied, lineups, placements, Some(obj_val))
        }
        OptimisationResult::Unsatisfiable => {
            return Ok(SolveResult {
                status: SolveStatus::Unsatisfiable,
                primary: ProposedSolution::default(),
                alternatives: vec![],
                diagnostics,
                elapsed: std::time::Duration::ZERO,
                objective: None,
            });
        }
        OptimisationResult::Unknown => {
            return Ok(SolveResult {
                status: SolveStatus::Timeout,
                primary: ProposedSolution::default(),
                alternatives: vec![],
                diagnostics: vec![],
                elapsed: std::time::Duration::ZERO,
                objective: None,
            });
        }
    };

    // Tabu re-solve loop for the remaining alternatives. Each
    // entry is a full `ProposedSolution` — lineups *and* its
    // own unplaced-rowers breakdown — so a coach comparing
    // alternatives sees both sides of each trade-off.
    let mut alternatives: Vec<ProposedSolution> = Vec::new();
    if request.wants_alternatives() {
        // Post the first tabu constraint off the primary
        // placements, then loop `top_n - 1` times. Each
        // iteration posts its own fresh tabu against the
        // solution it produced, so the N-th alternative differs
        // from all N-1 previous ones. If `post_tabu_constraint`
        // returns `false` at any point, the tabu radius is
        // larger than the placement set can support and we stop
        // looking.
        if post_tabu_constraint(
            &mut builder.solver,
            &primary_placements,
            request.tabu_min_diff,
        )? {
            for _ in 1..request.top_n {
                let next = run_one(&mut builder.solver, &mut brancher, &mut resolver);
                let sol = match next {
                    OptimisationResult::Optimal(sol)
                    | OptimisationResult::Satisfiable(sol)
                    | OptimisationResult::Stopped(sol, _) => sol,
                    // Unsat means the feasible region under the
                    // accumulated tabu constraints is empty —
                    // we've run out of distinct alternatives.
                    // Unknown means the solver timed out on this
                    // iteration; in that case we stop looking
                    // rather than returning a partial /
                    // potentially-duplicate result.
                    OptimisationResult::Unsatisfiable | OptimisationResult::Unknown => {
                        break;
                    }
                };

                let alt_lineups = decode_solution(
                    &builder.x,
                    &builder.use_b,
                    &builder.boats,
                    &builder.available,
                    |v| sol.get_integer_value(v),
                );
                let alt_placements = collect_placements(&builder.x, |v| sol.get_integer_value(v));
                let alt_unplaced = compute_unplaced(&builder.available, &alt_lineups);
                alternatives.push(ProposedSolution {
                    lineups: alt_lineups,
                    unplaced: alt_unplaced,
                });

                // Prepare the next iteration: forbid the set we
                // just returned. Skip the final iteration's tabu
                // post — it would only apply to a solve we're
                // not going to run. Stop early if
                // `post_tabu_constraint` signals that the radius
                // is exhausted.
                if alternatives.len() + 1 < request.top_n
                    && !post_tabu_constraint(
                        &mut builder.solver,
                        &alt_placements,
                        request.tabu_min_diff,
                    )?
                {
                    break;
                }
            }
        }
    }

    let primary_unplaced = compute_unplaced(&builder.available, &primary_lineups);
    tracing::debug!(benched = primary_unplaced.benched.len(), "unplaced rowers");
    Ok(SolveResult {
        status: primary_status,
        primary: ProposedSolution {
            lineups: primary_lineups,
            unplaced: primary_unplaced,
        },
        alternatives,
        diagnostics: vec![],
        elapsed: std::time::Duration::ZERO, // filled in by solve()
        objective: primary_objective,
    })
}

/// Bucket the available rowers into "placed in a lineup" and
/// "benched" given a set of ProposedLineups. The returned
/// `UnplacedRowers` preserves the
/// iteration order of `available` so repeated runs produce
/// identical output (important for the baseline regression
/// test).
fn compute_unplaced(available: &[&Rower], lineups: &[ProposedLineup]) -> UnplacedRowers {
    use std::collections::HashSet;
    let placed: HashSet<RowerId> = lineups
        .iter()
        .filter(|l| l.used)
        .flat_map(|l| l.seats.iter().map(|(_, r)| *r))
        .collect();
    let mut out = UnplacedRowers::default();
    for rower in available {
        if placed.contains(&rower.id) {
            continue;
        }
        out.benched.push(rower.id);
    }
    out
}

/// Walk the `x[(r, b, s)]` assignment matrix and return the
/// [`DomainId`]s of every variable that evaluated to 1 in the
/// given solution. Used by the tabu re-solve loop to build a
/// "forbid re-using too many of these placements" constraint.
fn collect_placements(
    x: &BTreeMap<(usize, usize, i32), DomainId>,
    mut value_of: impl FnMut(DomainId) -> i32,
) -> Vec<DomainId> {
    x.values().copied().filter(|&v| value_of(v) == 1).collect()
}

/// Post a tabu constraint forcing any future solution to differ
/// from the given `placements` set by at least `min_diff` seats.
///
/// Mathematically, given the previous solution's live-placement
/// set `P` of size `|P|`, we require
///
///   Σ_{v ∈ P} v ≤ |P| − min_diff
///
/// which says "at most `|P| − min_diff` of the previous placements
/// may repeat", equivalently "at least `min_diff` must flip".
///
/// Returns `Ok(true)` when the constraint was posted successfully
/// and the re-solve loop can continue; `Ok(false)` when `min_diff`
/// exceeds the total number of filled seats, in which case there
/// is no point posting anything — the request is trivially
/// infeasible and the caller should stop looking for more
/// alternatives. An empty placement set (e.g. a primary solve
/// that fielded no boats) is treated the same way.
fn post_tabu_constraint(
    solver: &mut Solver,
    placements: &[DomainId],
    min_diff: i32,
) -> Result<bool> {
    let cap = placements.len() as i32 - min_diff;
    if cap < 0 || placements.is_empty() {
        return Ok(false);
    }
    let terms: Vec<AffineView<DomainId>> =
        placements.iter().map(|v: &DomainId| v.scaled(1)).collect();
    let tag = solver.new_constraint_tag();
    solver
        .add_constraint(pumpkin_constraints::less_than_or_equals(terms, cap, tag))
        .post()
        .map_err(|e| anyhow!("tabu re-solve constraint: {e:?}"))?;
    Ok(true)
}

/// No-op solution callback for `LinearSatUnsat`. The
/// `LinearSatUnsat::new` API requires *some* callback type and
/// we don't need intermediate solution hooks — Top-N is
/// implemented via the tabu re-solve loop around `optimise()`,
/// not via an in-search callback, so every top-N iteration
/// re-uses this same no-op stub.
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
