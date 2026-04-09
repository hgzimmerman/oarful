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

use anyhow::{anyhow, bail, Result};
use chrono::NaiveDate;
use lineup_db::boat::{
    types::{BoatId, WeightClass},
    Boat,
};
use lineup_db::rower::{
    types::{RowerId, Side},
    Rower,
};
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
const COX_COOLDOWN_DAYS: i64 = 14;

// S6 cox-cooldown penalty and all other per-constraint weights are now
// controlled via `SolverConfig` on the SolveRequest — see the struct
// definition above. S7 novelty band width is separately controlled by
// `SolveRequest.novelty_factor`.

/// Which rowing seats of a given boat are "optional" — i.e. may be left
/// empty under a non-strict [`PartialFillPolicy`]. The set is hardcoded
/// per boat class based on common rowing practice:
///
/// - **8+**: seats 3 and 4 are the inside bow pair; these are the
///   conventional "row it down a pair" positions when the club is short
///   on rowers.
/// - **Everything else**: no optional seats. A 4-boat with a missing
///   seat is too unbalanced to be useful, and smaller boats have no
///   realistic partial-fill pattern.
fn optional_seats(boat: &Boat) -> Vec<i32> {
    match boat.seat_count {
        8 => vec![3, 4],
        _ => vec![],
    }
}

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

    // Per-constraint weights. Read once up front so the constraint
    // blocks below can check `cfg.foo_weight != 0` to skip disabled
    // constraints entirely (Pumpkin panics on `.scaled(0)`).
    let cfg = request.config;

    let mut solver = Solver::default();
    // x[(rower_idx, boat_idx, seat_position)] ∈ {0,1}
    let mut x: BTreeMap<(usize, usize, i32), DomainId> = BTreeMap::new();

    // use[boat_idx] ∈ {0,1} — whether the solver chose to field this
    // boat today. Drives boat selection (formerly a CLI-side greedy) and
    // is referenced by the conditional weight-class wall and S5 slack
    // equality so that unused boats don't generate phantom penalties.
    let use_b: Vec<DomainId> = boats
        .iter()
        .map(|_| solver.new_bounded_integer(0, 1))
        .collect();

    // Weighted objective terms. Each soft constraint appends one or more
    // pre-scaled `AffineView` terms here; at the end we link the objective
    // variable to their sum.
    let mut obj_terms: Vec<AffineView<DomainId>> = Vec::new();

    // S8: reward fielding each boat by its total seat count (rowers + cox).
    //
    // Because H1 is all-or-nothing per boat (use[b]=1 implies every seat
    // filled), rewarding "rowers placed" and rewarding "seat_total *
    // use[b]" are equivalent. The per-boat form pushes only N terms into
    // the objective-link equality, vs. one term per x variable (which
    // was ~100+ for our fixture and dramatically slowed Pumpkin's
    // propagation through the huge linear objective constraint).
    if cfg.placement_reward_weight != 0 {
        for (b_idx, boat) in boats.iter().enumerate() {
            let seats_total =
                boat.seat_count + if boat.has_cox.as_bool() { 1 } else { 0 };
            let coef = -seats_total * cfg.placement_reward_weight;
            if coef != 0 {
                obj_terms.push(use_b[b_idx].scaled(coef));
            }
        }
    }

    // --- Variables ---
    // A variable x[(r,b,s)] ∈ {0,1} is created only when rower r is eligible
    // for seat s of boat b. Eligibility rules:
    //   - seat 0 (cox): only rowers with `can_cox`
    //   - rowing seats: designated coxes rejected outright; other rowers
    //     are eligible if their side matches the seat's side, is
    //     `Either`, OR their `side_strength` is soft (non-hard), in which
    //     case wrong-side placement is a soft preference rather than a
    //     hard rule — see S4 aggregation below.
    //
    // While creating variables we also collect every wrong-side x per
    // rower into `wrong_side_by_rower`. S4 aggregation (below) turns
    // those into a single per-rower `wrong_count[r] ∈ {0,1}` aux var,
    // so the obj_terms count for S4 is O(rowers) rather than
    // O(rowers × seats). This matters: the S8 experiment taught us
    // that linear scan of the objective-link equality scales poorly
    // with term count, and a 40-rower club would otherwise push ~1000
    // terms into obj_terms just from S4.
    let mut wrong_side_by_rower: BTreeMap<usize, Vec<DomainId>> = BTreeMap::new();
    for (b_idx, boat) in boats.iter().enumerate() {
        for seat in seat_positions(boat) {
            for (r_idx, rower) in available.iter().enumerate() {
                if !rower_eligible_for_seat(rower, boat, seat) {
                    continue;
                }
                let var = solver.new_bounded_integer(0, 1);
                x.insert((r_idx, b_idx, seat), var);

                // S4: collect wrong-side placements for per-rower
                // aggregation rather than pushing a term per variable.
                if wrong_side_penalty(rower, boat, seat) > 0 {
                    wrong_side_by_rower.entry(r_idx).or_default().push(var);
                }
            }
        }
    }

    // --- S4: aggregate wrong-side placements per rower ---
    //
    // For each rower with any wrong-side candidate variable, create
    // `wrong_count[r] ∈ {0,1}` and post
    //    Σ wrong_side_x[r] - wrong_count[r] = 0
    // Because H2 guarantees the rower is in at most one seat total,
    // the sum is at most 1, so the [0,1] domain on wrong_count is
    // tight. Then push exactly one term into `obj_terms` per rower:
    //    wrong_count[r].scaled(rower.side_strength)
    // That's O(rowers) S4 terms instead of O(rowers × seats). Same
    // objective value, dramatically fewer terms in the final
    // objective-link equality. See crates/solver/README.md §S4 for
    // the detailed rationale and the performance note that motivated
    // this encoding.
    if cfg.side_preference_weight != 0 {
        for (&r_idx, wrong_vars) in &wrong_side_by_rower {
            if wrong_vars.is_empty() {
                continue;
            }
            let rower = available[r_idx];
            let coef = rower.side_strength.as_int() * cfg.side_preference_weight;
            if coef == 0 {
                continue; // stored strength = 0 already meant "hard lock" so this shouldn't fire
            }
            let wrong_count = solver.new_bounded_integer(0, 1);
            let mut link_terms: Vec<AffineView<DomainId>> =
                wrong_vars.iter().map(|v| v.scaled(1)).collect();
            link_terms.push(wrong_count.scaled(-1));
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(link_terms, 0, tag))
                .post()
                .map_err(|e| anyhow!("S4 wrong-side link: {e:?}"))?;

            obj_terms.push(wrong_count.scaled(coef));
        }
    }

    // --- S6: cox cooldown ---
    //
    // Non-designated rowers who coxed recently get a penalty if the
    // solver tries to seat them as cox again inside the cooldown
    // window. Designated coxes are exempt — they're meant to cox
    // often. The history data comes from `DbSnapshot.last_coxed`,
    // which the db layer derives from committed lineup_seat rows.
    //
    // Encoding mirrors S4 per-rower aggregation: collect the rower's
    // cox-seat x variables across all boats (each rower has at most
    // one such var per boat, since only coxed boats have seat 0),
    // sum them into a `cox_use[r] ∈ {0,1}` aux var (by H2 a rower is
    // in at most one seat overall), link via a linear equality, and
    // push ONE scaled penalty term into obj_terms. That's O(rowers in
    // cooldown) obj terms rather than O(coxed boats × rowers in
    // cooldown) — same pattern that made S4 and S8 tractable at
    // scale.
    //
    // The penalty is a flat `cfg.cox_cooldown_penalty` regardless of
    // how recent the last cox was, because (a) it keeps the encoding
    // simple and (b) decay-by-days would require either per-rower
    // coefficients (which we support) or a quadratic penalty (which
    // we don't). A follow-up could add a linear decay like
    // `penalty * (cooldown - days_since) / cooldown`.
    if cfg.cox_cooldown_penalty != 0 {
        for (r_idx, rower) in available.iter().enumerate() {
            if rower.is_designated_cox.as_bool() {
                continue; // exempt — designated coxes cox as often as needed
            }
            let Some(last_date) = snapshot.last_coxed.get(&rower.id) else {
                continue; // never coxed → no cooldown to enforce
            };
            let days_since = (request.date - *last_date).num_days();
            if days_since < 0 || days_since >= COX_COOLDOWN_DAYS {
                continue; // outside cooldown window (or in the future; ignore)
            }

            // Gather this rower's cox-seat x variables. Each coxed boat
            // contributes one; coxless boats don't create a seat-0 x var
            // in the first place, so there's nothing to collect there.
            let cox_vars: Vec<DomainId> = boats
                .iter()
                .enumerate()
                .filter_map(|(b_idx, _)| x.get(&(r_idx, b_idx, 0)).copied())
                .collect();

            if cox_vars.is_empty() {
                continue; // rower has no cox vars
            }

            let cox_use = solver.new_bounded_integer(0, 1);
            let mut link: Vec<AffineView<DomainId>> =
                cox_vars.iter().map(|v| v.scaled(1)).collect();
            link.push(cox_use.scaled(-1));
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(link, 0, tag))
                .post()
                .map_err(|e| anyhow!("S6 cox-use link: {e:?}"))?;

            obj_terms.push(cox_use.scaled(cfg.cox_cooldown_penalty));
        }
    }

    // --- S7: novelty vs recent lineups ---
    //
    // Penalise lineups that are too similar to recently-committed
    // ones. "Similarity" is counted per historical lineup (one
    // committed (practice, boat) pair) as the number of placements
    // that would match if the solver rowed the same boat again with
    // the same rowers in the same rowing seats.
    //
    // Controlled by `request.novelty_factor`:
    //
    //   0: no constraint. Exact repeats are fine. Special-cased —
    //      we don't even post the constraint.
    //   1: deprioritize lineups 1 seat (or fewer) different from any
    //      historical lineup. Exact repeats incur the largest
    //      penalty; "all but 1 seat same" incurs a smaller one.
    //   2: extends the band to 2-seat differences, and so on.
    //
    // Encoding per historical lineup L (with N_L rowing placements):
    //
    //   threshold = N_L - factor - 1
    //   match_L   = Σ x[r, b, s] for each of L's (rower, boat, seat)
    //               placements that still has a live x variable today
    //   penalty_L ≥ match_L - threshold           (soft lower bound)
    //   penalty_L ≥ 0                             (via domain)
    //   obj_terms.push(penalty_L.scaled(1))
    //
    // The first inequality is posted as
    //   Σ match_terms - penalty_L ≤ threshold
    // which gives `penalty_L ≥ match_L - threshold`. The solver
    // minimises, so it picks `max(0, match_L - threshold)` — zero
    // below threshold, linearly growing above.
    //
    // Numerical check for factor = 1 on an N = 8 lineup:
    //   threshold = 6
    //   match = 8 (exact)  →  penalty = 2
    //   match = 7          →  penalty = 1
    //   match = 6          →  penalty = 0
    //
    // For factor = 2 on N = 8:
    //   threshold = 5
    //   match = 8  →  penalty = 3
    //   match = 7  →  penalty = 2
    //   match = 6  →  penalty = 1
    //
    // Cox seats are deliberately excluded. Cox rotation is governed
    // by S6 cox cooldown, which has a designated-exempt case that S7
    // would fight against. Rowers who are no longer available today
    // or boats no longer in today's candidate fleet contribute
    // nothing — their x variables don't exist, so the match sum
    // just skips those placements and the historical lineup appears
    // "smaller" than it was (which is correct: we can't reproduce
    // placements we can no longer make).
    //
    // Cost budget: one aux var + one linear inequality per historical
    // lineup. With RECENT_LINEUP_WINDOW = 4 and a realistic fleet,
    // that's at most ~16 historical lineups. Tiny.
    if request.novelty_factor > 0 && cfg.novelty_weight != 0 {
        // Group recent placements by (practice_date, boat_id). Each
        // group is one historical lineup whose similarity to the
        // current assignment we want to penalise.
        let mut groups: BTreeMap<
            (NaiveDate, BoatId),
            Vec<&lineup_db::lineup::RecentPlacement>,
        > = BTreeMap::new();
        for placement in &snapshot.recent_placements {
            if placement.is_cox || placement.seat_position == 0 {
                continue; // cox rotation handled by S6
            }
            groups
                .entry((placement.practice_date, placement.boat_id))
                .or_default()
                .push(placement);
        }

        for placements in groups.values() {
            // Match terms: x variables for placements that still
            // exist in today's model. Placements whose rower is
            // absent / boat is out of the fleet / x var doesn't
            // exist are silently dropped.
            let mut match_terms: Vec<AffineView<DomainId>> = Vec::new();
            for p in placements {
                let Some(r_idx) = available.iter().position(|r| r.id == p.rower_id)
                else {
                    continue;
                };
                let Some(b_idx) = boats.iter().position(|b| b.id == p.boat_id) else {
                    continue;
                };
                if let Some(&var) = x.get(&(r_idx, b_idx, p.seat_position)) {
                    match_terms.push(var.scaled(1));
                }
            }

            let reachable_matches = match_terms.len() as i32;
            if reachable_matches == 0 {
                continue; // nothing from this historical lineup is live
            }

            let threshold = reachable_matches - request.novelty_factor - 1;
            // If the threshold is ≥ max possible match count, the
            // constraint is trivially slack (penalty always 0) — skip
            // posting it to save Pumpkin work.
            if threshold >= reachable_matches {
                continue;
            }

            // penalty upper bound: max possible is `reachable_matches
            // - threshold` = `factor + 1`. Overshoot slightly for
            // safety.
            let penalty_upper = request.novelty_factor + 2;
            let penalty = solver.new_bounded_integer(0, penalty_upper);

            // Σ match_terms - penalty ≤ threshold
            //   ⇔  penalty ≥ Σ match_terms - threshold
            let mut lhs = match_terms.clone();
            lhs.push(penalty.scaled(-1));
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::less_than_or_equals(
                    lhs, threshold, tag,
                ))
                .post()
                .map_err(|e| anyhow!("S7 novelty link: {e:?}"))?;

            obj_terms.push(penalty.scaled(cfg.novelty_weight));
        }
    }

    // --- Hard constraint 1: seat fill conditional on `use[b]`. ---
    //
    // For each REQUIRED (boat, seat):   Σ_r x[r,b,s] = use[b]
    // For each OPTIONAL (boat, seat):   Σ_r x[r,b,s] ≤ use[b]
    //
    // Required seats must be filled whenever the boat is used; optional
    // seats may be empty (but still can't be double-filled). The
    // partial-fill policy adds a cap on how many optional seats may go
    // empty — see the cap posting below.
    //
    // If the solver picks the boat (use[b] = 1) every required seat is
    // filled exactly once and every optional seat is 0 or 1; if not
    // (use[b] = 0) every seat is empty. This all-or-nothing-per-boat
    // semantics is what lets boat selection move inside the model —
    // the solver decides use[b] based on the objective balance.
    let k_allowed = request.partial_fill.max_empty();
    for (b_idx, boat) in boats.iter().enumerate() {
        let opt_seats = optional_seats(boat);
        for seat in seat_positions(boat) {
            let mut terms: Vec<AffineView<DomainId>> = (0..available.len())
                .filter_map(|r_idx| x.get(&(r_idx, b_idx, seat)).map(|v| v.scaled(1)))
                .collect();
            if terms.is_empty() {
                // If no rower is eligible for this seat at all, the boat
                // can never be used. Force use[b] = 0.
                let tag = solver.new_constraint_tag();
                solver
                    .add_constraint(pumpkin_constraints::equals(
                        vec![use_b[b_idx].scaled(1)],
                        0,
                        tag,
                    ))
                    .post()
                    .map_err(|e| anyhow!("posting boat-unusable constraint: {e:?}"))?;
                tracing::debug!(
                    boat = %boat.name,
                    seat,
                    "no eligible rower for seat; forcing boat unused"
                );
                break;
            }
            // Required: Σ x - use[b] = 0. Optional: Σ x - use[b] ≤ 0.
            terms.push(use_b[b_idx].scaled(-1));
            let tag = solver.new_constraint_tag();
            if opt_seats.contains(&seat) && k_allowed > 0 {
                solver
                    .add_constraint(pumpkin_constraints::less_than_or_equals(
                        terms, 0, tag,
                    ))
                    .post()
                    .map_err(|e| anyhow!("posting optional seat-fill constraint: {e:?}"))?;
            } else {
                solver
                    .add_constraint(pumpkin_constraints::equals(terms, 0, tag))
                    .post()
                    .map_err(|e| anyhow!("posting seat-fill constraint: {e:?}"))?;
            }
        }

        // Partial-fill cap: at least `(n_opt - k_allowed)` of the
        // optional seats must be filled when the boat is used.
        //
        //   Σ_{s ∈ opt_seats, r} x[r,b,s]  ≥  (n_opt - k) * use[b]
        //   ⇔  (n_opt - k) * use[b] - Σ x ≤ 0
        //
        // Only posted when k > 0 and the boat has optional seats;
        // otherwise the tight H1 equality above already forces every
        // optional seat to be filled.
        let n_opt = opt_seats.len() as i32;
        if k_allowed > 0 && n_opt > 0 {
            let k = k_allowed.min(n_opt);
            let min_filled_opt = n_opt - k;
            if min_filled_opt > 0 {
                let mut cap_terms: Vec<AffineView<DomainId>> = Vec::new();
                for s in &opt_seats {
                    for r_idx in 0..available.len() {
                        if let Some(&var) = x.get(&(r_idx, b_idx, *s)) {
                            cap_terms.push(var.scaled(-1));
                        }
                    }
                }
                cap_terms.push(use_b[b_idx].scaled(min_filled_opt));
                let tag = solver.new_constraint_tag();
                solver
                    .add_constraint(pumpkin_constraints::less_than_or_equals(
                        cap_terms, 0, tag,
                    ))
                    .post()
                    .map_err(|e| anyhow!("posting partial-fill cap: {e:?}"))?;
            }
        }
    }

    // --- Hard constraint 2: each rower occupies at most one seat total. ---
    for r_idx in 0..available.len() {
        let terms: Vec<DomainId> = x
            .iter()
            .filter_map(|(&(r, _, _), &v)| if r == r_idx { Some(v) } else { None })
            .collect();
        if terms.is_empty() {
            continue;
        }
        let tag = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::less_than_or_equals(terms, 1, tag))
            .post()
            .map_err(|e| anyhow!("posting rower-at-most-one constraint: {e:?}"))?;
    }

    // --- Hard constraint 6: fleet capacity bound ---
    //
    // The total number of seats across all fielded boats cannot exceed
    // the number of available rowers. This is a direct global bound on
    // the fleet-selection search space:
    //
    //   Σ_b (seats_total[b] · use[b])  ≤  num_available
    //
    // Without this, the solver explores many infeasible fleet
    // configurations (e.g. "field 6 eights = 54 seats" with only 20
    // rowers available) before the individual H1/H2 constraints prune
    // them. The explicit global bound collapses the search drastically
    // and is the difference between "30s timeout at 5+ candidate boats"
    // and "sub-second solve at 10 candidate boats" on the benchmark.
    //
    // The bound is necessary but not sufficient — side, cox, and
    // weight-class constraints still apply on top. It's a cheap prune,
    // not a complete feasibility check.
    {
        let capacity_terms: Vec<_> = boats
            .iter()
            .enumerate()
            .map(|(b_idx, boat)| {
                let seats_total =
                    boat.seat_count + if boat.has_cox.as_bool() { 1 } else { 0 };
                use_b[b_idx].scaled(seats_total)
            })
            .collect();
        let tag = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::less_than_or_equals(
                capacity_terms,
                available.len() as i32,
                tag,
            ))
            .post()
            .map_err(|e| anyhow!("posting fleet-capacity bound: {e:?}"))?;
    }

    // --- H5 + S5: weight-class hard wall (upper only) + soft target ---
    //
    // We keep an UNCONDITIONAL upper bound on the ordinal sum — sum ≤
    // target_sum + N — which is trivially satisfied when the boat is
    // unused (sum = 0). No conditioning needed, no big-M. The upper
    // bound catches "Light-rigged boat full of Heavies" outright.
    //
    // The LOWER bound ("not too light") is intentionally dropped as a
    // hard rule. The former big-M-conditioned constraint
    // (sum ≥ (target - N) * use[b]) caused dramatic propagation slowdown
    // in Pumpkin — big-M formulations weaken CP propagation and blew
    // solve time from milliseconds to tens of seconds. S5's soft slack
    // penalty is sufficient to discourage too-light crews: fielding a
    // Heavy boat with Lights costs a large `under` slack which almost
    // always exceeds the S8 placement reward.
    //
    // S5 SOFT TARGET (conditional on use[b]):
    //   sum(ordinal*x) - over[b] + under[b] = target_sum * use[b]
    // At optimum, over = under = 0 when use[b] = 0.
    for (b_idx, boat) in boats.iter().enumerate() {
        let n_rowing = boat.seat_count;
        if n_rowing == 0 {
            continue;
        }
        let target = boat_target_weight_ordinal(boat.weight_class);
        let target_sum = target * n_rowing;
        let wall = n_rowing;

        let positive_terms: Vec<_> = x
            .iter()
            .filter_map(|(&(r_idx, b, seat), &var)| {
                if b != b_idx || seat == 0 {
                    return None;
                }
                Some(var.scaled(available[r_idx].weight_class.ordinal()))
            })
            .collect();
        if positive_terms.is_empty() {
            continue;
        }

        // Hard wall UPPER (unconditional).
        let tag_hi = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::less_than_or_equals(
                positive_terms.clone(),
                target_sum + wall,
                tag_hi,
            ))
            .post()
            .map_err(|e| anyhow!("weight-class hard wall (upper): {e:?}"))?;

        // S5 slack: sum(ordinal*x) - over + under - target_sum*use[b] = 0
        //
        // Only posted when the slack contributes to the objective.
        // If `weight_class_slack_weight == 0`, the caller has
        // disabled the soft target entirely — the hard wall above
        // still applies, but the solver has no preference between
        // any two configurations that both satisfy it.
        if cfg.weight_class_slack_weight != 0 {
            let slack_upper = 3 * n_rowing;
            let over = solver.new_bounded_integer(0, slack_upper);
            let under = solver.new_bounded_integer(0, slack_upper);

            let mut eq_terms: Vec<_> = positive_terms.clone();
            eq_terms.push(over.scaled(-1));
            eq_terms.push(under.scaled(1));
            eq_terms.push(use_b[b_idx].scaled(-target_sum));
            let tag_eq = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(eq_terms, 0, tag_eq))
                .post()
                .map_err(|e| anyhow!("weight-class slack equality: {e:?}"))?;

            obj_terms.push(over.scaled(cfg.weight_class_slack_weight));
            obj_terms.push(under.scaled(cfg.weight_class_slack_weight));
        }
    }

    // --- Shared per-seat trait aggregation ---
    //
    // Several soft constraints need "the skill / strength / height
    // ordinal of whoever ends up in this required rowing seat" as a
    // DomainId: S1 (skill spread per boat), S9 (pair strength diff),
    // S10 (pair height diff), S11 (end-pair skill bonus), S12
    // (engine-room strength bonus). Rather than building the
    // per-(boat, seat) aux var + link equality independently inside
    // each block — which duplicates work and wastes both aux vars
    // and propagation — we build each trait map exactly once here,
    // gated on whether *any* of its consumer constraints is enabled.
    //
    // Optional (partial-fill) seats are excluded: those may legally
    // be empty under `PartialFillPolicy::Allowed`, so their seat-trait
    // var would equal 0 and poison pair diffs / spread calculations.
    // This matches the original per-block behaviour.
    let seat_skill_by_seat: BTreeMap<(usize, i32), DomainId> =
        if cfg.skill_variance_weight != 0 || cfg.end_pair_skill_weight != 0 {
            build_seat_trait_map(
                &mut solver,
                &boats,
                &available,
                &x,
                |r| r.skill.ordinal(),
                "seat skill link",
            )?
        } else {
            BTreeMap::new()
        };

    let seat_strength_by_seat: BTreeMap<(usize, i32), DomainId> =
        if cfg.pair_strength_weight != 0 || cfg.engine_room_strength_weight != 0 {
            build_seat_trait_map(
                &mut solver,
                &boats,
                &available,
                &x,
                |r| r.strength.ordinal(),
                "seat strength link",
            )?
        } else {
            BTreeMap::new()
        };

    let seat_height_by_seat: BTreeMap<(usize, i32), DomainId> =
        if cfg.height_balance_weight != 0 {
            build_seat_trait_map(
                &mut solver,
                &boats,
                &available,
                &x,
                |r| r.height.ordinal(),
                "seat height link",
            )?
        } else {
            BTreeMap::new()
        };

    // --- S1: skill variance per boat ---
    //
    // We penalise large skill spread within a boat (don't mix a lone
    // expert with seven novices). Encoding:
    //   1. Per rowing seat, create `seat_skill[b,s] ∈ [0,3]` and link it
    //      to the assignment via
    //        Σ_r skill_ordinal(r) · x[r,b,s] - seat_skill[b,s] = 0
    //      Because exactly one x is 1 per filled seat (H1), the seat_skill
    //      variable equals the placed rower's skill ordinal.
    //   2. Compute `boat_max[b]` and `boat_min[b]` via Pumpkin's
    //      `maximum` / `minimum` global constraints over the seat_skill
    //      vars for each boat.
    //   3. `spread[b] = boat_max - boat_min` via a linear equality,
    //      bounded in [0, 3] (ordinal range).
    //   4. Push `spread[b]` into `slack_vars` so it rides the same
    //      minimisation pipeline as the S5 weight-class slacks.
    //
    // Each unit of skill spread contributes `cfg.skill_variance_weight`
    // to the objective. Setting the weight to 0 skips the entire block —
    // no per-seat aux vars, no max/min constraints, no obj push.
    if cfg.skill_variance_weight != 0 {
    for (b_idx, boat) in boats.iter().enumerate() {
        let n_rowing = boat.seat_count;
        if n_rowing == 0 {
            continue;
        }

        // The shared `seat_skill_by_seat` map already excludes optional
        // (partial-fill) seats. Collect this boat's required rowing
        // seats in order — max/min operates on values, so seat order
        // doesn't matter beyond stability for debugging.
        let seat_skill_vars: Vec<DomainId> = (1..=n_rowing)
            .filter_map(|seat| seat_skill_by_seat.get(&(b_idx, seat)).copied())
            .collect();

        if seat_skill_vars.len() < 2 {
            continue; // single-seat (or empty) boat — no meaningful spread
        }

        let boat_max = solver.new_bounded_integer(0, 4);
        let boat_min = solver.new_bounded_integer(0, 4);

        let tag_max = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::maximum(
                seat_skill_vars.clone(),
                boat_max,
                tag_max,
            ))
            .post()
            .map_err(|e| anyhow!("boat skill max: {e:?}"))?;

        let tag_min = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::minimum(
                seat_skill_vars,
                boat_min,
                tag_min,
            ))
            .post()
            .map_err(|e| anyhow!("boat skill min: {e:?}"))?;

        let spread = solver.new_bounded_integer(0, 3);
        // boat_max - boat_min - spread = 0  ⇔  spread = boat_max - boat_min
        let tag_spread = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::equals(
                vec![boat_max.scaled(1), boat_min.scaled(-1), spread.scaled(-1)],
                0,
                tag_spread,
            ))
            .post()
            .map_err(|e| anyhow!("spread link: {e:?}"))?;

        obj_terms.push(spread.scaled(cfg.skill_variance_weight));
    }
    }

    // --- S2: pair affinities ---
    //
    // A "pair" in rowing is a fixed 2-seat partition of a boat: seats
    // (1,2), (3,4), (5,6), (7,8). Under standard alternating rig each
    // partition contains one port and one starboard rower. We encode
    // `pair_affinity(A, B, w)` as a per-partition reified boolean
    // `together[pair, boat, partition] ∈ {0,1}` driven by the AND of
    // "A is in this partition" and "B is in this partition":
    //
    //   A_in_part = x[A, b, s_lo] + x[A, b, s_hi]   (at most 1)
    //   B_in_part = x[B, b, s_lo] + x[B, b, s_hi]   (at most 1)
    //   together ≤ A_in_part
    //   together ≤ B_in_part
    //   together ≥ A_in_part + B_in_part - 1
    //
    // `together.scaled(-w)` is pushed into `obj_terms`. Positive w
    // rewards pair-sharing; negative w penalises it. Unavailable
    // rowers, designated coxes, and bucket-rigged boats all yield
    // structurally-zero indicators, so those cases are inert rather
    // than erroring.
    //
    // Non-standard rigs: this encoding assumes standard alternating rig
    // (see README §Scope). Double-bucket rigs break the "pair contains
    // one port + one starboard" invariant — a pair in such a boat is
    // more of a convention than a structural guarantee. The affinity
    // still fires if both rowers land in the same 2-seat partition,
    // but the "one-port-one-starboard" expectation doesn't hold.
    if cfg.pair_affinity_weight != 0 {
    for aff in &snapshot.pair_affinities {
        // AffinityWeight forbids 0 at construction, but keep the guard
        // so a manually-crafted zero (e.g. from a future DB patch path
        // that bypassed the constructor) doesn't panic in Pumpkin.
        if aff.weight.as_int() == 0 {
            continue;
        }
        let a_idx = match available.iter().position(|r| r.id == aff.rower_a_id) {
            Some(i) => i,
            None => continue,
        };
        let b_idx = match available.iter().position(|r| r.id == aff.rower_b_id) {
            Some(i) => i,
            None => continue,
        };

        for (boat_idx, boat) in boats.iter().enumerate() {
            // Iterate pair partitions: (1,2), (3,4), (5,6), (7,8) ...
            let mut s_lo = 1;
            while s_lo + 1 <= boat.seat_count {
                let s_hi = s_lo + 1;

                // A_in_part and B_in_part: each a Vec of up to 2 x vars.
                let a_terms: Vec<_> = [s_lo, s_hi]
                    .into_iter()
                    .filter_map(|s| x.get(&(a_idx, boat_idx, s)).copied())
                    .collect();
                let b_terms: Vec<_> = [s_lo, s_hi]
                    .into_iter()
                    .filter_map(|s| x.get(&(b_idx, boat_idx, s)).copied())
                    .collect();

                // If either rower has no eligible variable for both seats
                // of this partition (e.g. they're a designated cox, or
                // side-locked away from both seats), the partition is
                // structurally infeasible for them — skip and leave the
                // affinity inert for this (boat, partition).
                if a_terms.is_empty() || b_terms.is_empty() {
                    s_lo += 2;
                    continue;
                }

                let together = solver.new_bounded_integer(0, 1);

                // together ≤ A_in_part  ⇔  together - A_in_part ≤ 0
                let mut upper_a = vec![together.scaled(1)];
                for t in &a_terms {
                    upper_a.push(t.scaled(-1));
                }
                let tag = solver.new_constraint_tag();
                solver
                    .add_constraint(pumpkin_constraints::less_than_or_equals(
                        upper_a, 0, tag,
                    ))
                    .post()
                    .map_err(|e| anyhow!("pair reif upper-A: {e:?}"))?;

                // together ≤ B_in_part
                let mut upper_b = vec![together.scaled(1)];
                for t in &b_terms {
                    upper_b.push(t.scaled(-1));
                }
                let tag = solver.new_constraint_tag();
                solver
                    .add_constraint(pumpkin_constraints::less_than_or_equals(
                        upper_b, 0, tag,
                    ))
                    .post()
                    .map_err(|e| anyhow!("pair reif upper-B: {e:?}"))?;

                // together ≥ A_in_part + B_in_part - 1
                //   ⇔ -together + A_in_part + B_in_part ≤ 1
                let mut lower = vec![together.scaled(-1)];
                for t in &a_terms {
                    lower.push(t.scaled(1));
                }
                for t in &b_terms {
                    lower.push(t.scaled(1));
                }
                let tag = solver.new_constraint_tag();
                solver
                    .add_constraint(pumpkin_constraints::less_than_or_equals(
                        lower, 1, tag,
                    ))
                    .post()
                    .map_err(|e| anyhow!("pair reif lower: {e:?}"))?;

                obj_terms
                    .push(together.scaled(-aff.weight.as_int() * cfg.pair_affinity_weight));

                s_lo += 2;
            }
        }
    }
    }

    // --- S3: seat affinities ---
    //
    // For each stored (rower, seat_position, weight) entry, push a
    // `x[r,b,seat_position].scaled(-weight)` term into `obj_terms` for
    // every boat that has a matching seat position. The negation flips
    // "reward for being there" into "negative contribution to a
    // minimised objective". Negative stored weights (dislike / avoid)
    // become positive contributions and act as penalties.
    //
    // seat_position is boat-agnostic: a preference for seat 4 applies to
    // every boat where seat 4 exists (so a stroke-4 preference applies
    // to 4-boats but not 8-boats, and vice versa).
    if cfg.seat_affinity_weight != 0 {
    for aff in &snapshot.seat_affinities {
        // AffinityWeight forbids 0 at construction and at the SQL
        // CHECK, so this guard is belt-and-braces to keep a future
        // malformed row from panicking Pumpkin via `.scaled(0)`.
        if aff.weight.as_int() == 0 {
            continue;
        }
        let r_idx = match available.iter().position(|r| r.id == aff.rower_id) {
            Some(i) => i,
            None => continue, // unavailable today or filtered out
        };
        for (b_idx, boat) in boats.iter().enumerate() {
            if aff.seat_position < 1 || aff.seat_position > boat.seat_count {
                continue; // this boat doesn't have that seat
            }
            if let Some(&var) = x.get(&(r_idx, b_idx, aff.seat_position)) {
                obj_terms
                    .push(var.scaled(-aff.weight.as_int() * cfg.seat_affinity_weight));
            }
        }
    }
    }

    // --- S9: pair strength balance ---
    //
    // Within a single rowing pair (a 2-seat partition), the two rowers
    // should have similar strength. A mismatched pair pulls harder on
    // one side and the boat yaws off course — matched strength means
    // the boat tracks straight. This is a universal structural rule,
    // not a coach preference about specific rowers, so it applies to
    // every partition regardless of the pair_affinity table.
    //
    // Encoding mirrors S1 but scoped to two-seat windows:
    //   1. Per rowing seat, `seat_strength[b,s] ∈ [1,4]` linked to
    //      `Σ_r ordinal(rower.strength) · x[r,b,s]`. H1 guarantees the
    //      sum equals the placed rower's strength ordinal.
    //   2. For each partition (s_lo, s_hi), compute
    //      `pair_max`, `pair_min` via maximum / minimum over the two
    //      seat_strength vars, then `diff = pair_max - pair_min`.
    //   3. Push `diff.scaled(1)` into `obj_terms`.
    //
    // Scaling note: Strength ordinals start at 1 (Weak=1 .. VeryStrong=4)
    // so `.scaled(ordinal)` never hits the Pumpkin zero-coefficient
    // panic. Spread is `max - min` and is invariant under the shift.
    if cfg.pair_strength_weight != 0 {
    for (b_idx, boat) in boats.iter().enumerate() {
        let n_rowing = boat.seat_count;
        if n_rowing < 2 {
            continue;
        }

        // Iterate 2-seat partitions and penalise strength spread. The
        // shared `seat_strength_by_seat` map already excludes optional
        // seats, so any partition containing an optional seat will
        // have a `None` lookup and skip. This keeps partial-fill-
        // capable partitions out of the pair-balance objective.
        let mut s_lo = 1i32;
        while s_lo + 1 <= n_rowing {
            let s_hi = s_lo + 1;
            let (lo_var, hi_var) = match (
                seat_strength_by_seat.get(&(b_idx, s_lo)).copied(),
                seat_strength_by_seat.get(&(b_idx, s_hi)).copied(),
            ) {
                (Some(l), Some(h)) => (l, h),
                _ => {
                    s_lo += 2;
                    continue;
                }
            };

            let pair_max = solver.new_bounded_integer(0, 4);
            let pair_min = solver.new_bounded_integer(0, 4);

            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::maximum(
                    vec![lo_var, hi_var],
                    pair_max,
                    tag,
                ))
                .post()
                .map_err(|e| anyhow!("pair strength max: {e:?}"))?;

            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::minimum(
                    vec![lo_var, hi_var],
                    pair_min,
                    tag,
                ))
                .post()
                .map_err(|e| anyhow!("pair strength min: {e:?}"))?;

            let diff = solver.new_bounded_integer(0, 3);
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(
                    vec![pair_max.scaled(1), pair_min.scaled(-1), diff.scaled(-1)],
                    0,
                    tag,
                ))
                .post()
                .map_err(|e| anyhow!("pair strength diff: {e:?}"))?;

            obj_terms.push(diff.scaled(cfg.pair_strength_weight));

            // S9b: the bow pair (seats 1, 2) has outsized influence on
            // set and steering, so we layer an extra diff term on top
            // of the regular S9 contribution for that partition only.
            // Safe to push the same `diff` DomainId twice — Pumpkin's
            // AffineView is a Copy projection of the underlying var,
            // and the objective-link equality just accumulates both
            // coefficients.
            if s_lo == 1 && cfg.bow_pair_strength_weight != 0 {
                obj_terms.push(diff.scaled(cfg.bow_pair_strength_weight));
            }

            s_lo += 2;
        }
    }
    }

    // --- S10: pair height balance ---
    //
    // Within a 2-seat partition we'd rather keep rowers of similar
    // height together. A pair of a Short with a VeryTall rows fine,
    // but alignment / rigging / catch timing all feel nicer when the
    // two are close in height. This is a gentle preference rather
    // than a hard structural rule, so the default weight is 1 (the
    // same as S1 skill variance) and the encoding intentionally
    // mirrors S9 so the two can ride the same shared
    // `seat_height_by_seat` map and the same partition iteration
    // pattern.
    //
    // Encoding per partition (s_lo, s_hi):
    //   pair_max = max(seat_height[b, s_lo], seat_height[b, s_hi])
    //   pair_min = min(seat_height[b, s_lo], seat_height[b, s_hi])
    //   diff     = pair_max - pair_min    (in [0, 3])
    //   obj_terms.push(diff.scaled(height_balance_weight))
    //
    // Optional seats are already excluded by the shared map (see
    // `build_seat_trait_map`), so partitions containing an optional
    // seat miss the lookup and skip — same as S9.
    if cfg.height_balance_weight != 0 {
    for (b_idx, boat) in boats.iter().enumerate() {
        let n_rowing = boat.seat_count;
        if n_rowing < 2 {
            continue;
        }

        let mut s_lo = 1i32;
        while s_lo + 1 <= n_rowing {
            let s_hi = s_lo + 1;
            let (lo_var, hi_var) = match (
                seat_height_by_seat.get(&(b_idx, s_lo)).copied(),
                seat_height_by_seat.get(&(b_idx, s_hi)).copied(),
            ) {
                (Some(l), Some(h)) => (l, h),
                _ => {
                    s_lo += 2;
                    continue;
                }
            };

            let pair_max = solver.new_bounded_integer(0, 4);
            let pair_min = solver.new_bounded_integer(0, 4);

            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::maximum(
                    vec![lo_var, hi_var],
                    pair_max,
                    tag,
                ))
                .post()
                .map_err(|e| anyhow!("pair height max: {e:?}"))?;

            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::minimum(
                    vec![lo_var, hi_var],
                    pair_min,
                    tag,
                ))
                .post()
                .map_err(|e| anyhow!("pair height min: {e:?}"))?;

            let diff = solver.new_bounded_integer(0, 3);
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(
                    vec![pair_max.scaled(1), pair_min.scaled(-1), diff.scaled(-1)],
                    0,
                    tag,
                ))
                .post()
                .map_err(|e| anyhow!("pair height diff: {e:?}"))?;

            obj_terms.push(diff.scaled(cfg.height_balance_weight));

            s_lo += 2;
        }
    }
    }

    // --- S11: end-pair skill reward (8-boats only) ---
    //
    // In an eight, seats 1/2 (the bow pair) and 7/8 (the stern pair)
    // are both high-skill positions but for different reasons. The
    // stern pair leads the stroke and sets rhythm for the rest of the
    // crew; the bow pair has the biggest influence on set and
    // steering — balance problems and course corrections both
    // originate there. Both jobs reward skill more than raw power,
    // so we nudge the solver to put the most technically skilled
    // rowers in those four seats.
    //
    // (The bow pair's *strength-balance* sensitivity — distinct from
    // skill — is handled separately by S9b
    // `bow_pair_strength_weight`, which layers an extra pair-balance
    // term on the (1, 2) partition of every boat.)
    //
    // Encoding: push a negative-coefficient term on each end-pair
    // `seat_skill` var. The objective is minimised, so a
    // `-weight * seat_skill` term is maximised at seat_skill = 4
    // (Expert). For unused boats every `x` is 0 → seat_skill = 0,
    // so the term contributes 0 and there's no phantom reward for
    // benching a boat.
    //
    // Only applies to 8-boats. Smaller boats have no meaningful
    // "engine room vs ends" split — a four is all engine, a pair is
    // all ends.
    if cfg.end_pair_skill_weight != 0 {
        const END_PAIR_SEATS: [i32; 4] = [1, 2, 7, 8];
        for (b_idx, boat) in boats.iter().enumerate() {
            if boat.seat_count != 8 {
                continue;
            }
            for seat in END_PAIR_SEATS {
                if let Some(&s_var) = seat_skill_by_seat.get(&(b_idx, seat)) {
                    obj_terms.push(s_var.scaled(-cfg.end_pair_skill_weight));
                }
            }
        }
    }

    // --- S12: engine-room strength reward (8-boats only) ---
    //
    // Seats 3/4/5/6 of an eight are the "engine room" — the four
    // middle seats do the bulk of the propulsive work. We reward
    // placing the strongest rowers there with a negative-coefficient
    // term on the engine-room `seat_strength` vars, exactly
    // symmetric to S11.
    //
    // Only applies to 8-boats: a four has no engine-room/ends
    // distinction, and smaller boats are entirely ends.
    if cfg.engine_room_strength_weight != 0 {
        const ENGINE_ROOM_SEATS: [i32; 4] = [3, 4, 5, 6];
        for (b_idx, boat) in boats.iter().enumerate() {
            if boat.seat_count != 8 {
                continue;
            }
            for seat in ENGINE_ROOM_SEATS {
                if let Some(&s_var) = seat_strength_by_seat.get(&(b_idx, seat)) {
                    obj_terms.push(s_var.scaled(-cfg.engine_room_strength_weight));
                }
            }
        }
    }

    // --- Objective variable ---
    // The objective is the sum of every weighted term pushed into
    // `obj_terms`: S5 weight deviation, S1 skill spread, S4 side-pref
    // penalty. Later soft constraints (S2, S3, S6, S7, S8) append to the
    // same vec with their own per-term weights.
    //
    // A generous range is fine here — Pumpkin will propagate tighter
    // bounds from the term domains during search. The lower bound must
    // be negative because S3 affinity rewards contribute negative terms
    // to the sum.
    let objective = solver.new_bounded_integer(-10_000, 10_000);
    if !obj_terms.is_empty() {
        let mut link_terms = obj_terms.clone();
        link_terms.push(objective.scaled(-1));
        let tag = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::equals(link_terms, 0, tag))
            .post()
            .map_err(|e| anyhow!("objective link: {e:?}"))?;
    }

    // --- Solve (optimisation) ---
    let mut brancher = solver.default_brancher();
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
            solver.optimise(&mut brancher, &mut termination, &mut resolver, procedure)
        }
        Some(budget) => {
            let mut termination = TimeBudget::starting_now(budget);
            let procedure =
                LinearSatUnsat::new(OptimisationDirection::Minimise, objective, NoCallback);
            solver.optimise(&mut brancher, &mut termination, &mut resolver, procedure)
        }
    };

    let result: SolveResult = match opt_result {
        OptimisationResult::Optimal(sol) | OptimisationResult::Satisfiable(sol) => {
            // Both branches return a concrete `Solution` rather than a
            // reference, so we can decode without borrow gymnastics.
            let lineups = decode_solution(&x, &use_b, &boats, &available, |v| {
                sol.get_integer_value(v)
            });
            SolveResult {
                status: SolveStatus::Satisfied,
                lineups,
            }
        }
        OptimisationResult::Stopped(sol, _) => {
            let lineups = decode_solution(&x, &use_b, &boats, &available, |v| {
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

/// Build a shared `(boat_idx, seat) -> seat_trait_var` map for a
/// per-rower ordinal (skill, strength, height). For each required
/// rowing seat of each boat, creates a `[0, 4]` aux var and posts
/// the link equality
///
///   Σ_r ordinal(rower_r) · x[r, b, seat] − seat_trait[b, seat] = 0
///
/// Because H1 guarantees the per-seat Σ_r x equals `use[b]`, the
/// seat_trait variable equals the placed rower's ordinal when the
/// seat is filled, and 0 when the boat is unused (the `[0, 4]`
/// domain is deliberately loose to accommodate the unused-boat case,
/// matching the per-block behaviour this helper replaced).
///
/// Optional (partial-fill) seats are excluded — an empty seat would
/// drive the trait var to 0 and pollute any downstream spread / diff
/// calculation. Consumers that care about partial-fill-friendly
/// per-partition logic must either work around the missing entries
/// or accept "we don't score that partition".
///
/// The caller is responsible for only calling this when at least
/// one consuming soft constraint is enabled — empty traits are
/// wasted aux vars + propagation work.
fn build_seat_trait_map(
    solver: &mut Solver,
    boats: &[&Boat],
    available: &[&Rower],
    x: &BTreeMap<(usize, usize, i32), DomainId>,
    ordinal: impl Fn(&Rower) -> i32,
    label: &'static str,
) -> Result<BTreeMap<(usize, i32), DomainId>> {
    let mut map: BTreeMap<(usize, i32), DomainId> = BTreeMap::new();
    for (b_idx, boat) in boats.iter().enumerate() {
        let n_rowing = boat.seat_count;
        if n_rowing == 0 {
            continue;
        }
        let opt_seats = optional_seats(boat);
        for seat in 1..=n_rowing {
            if opt_seats.contains(&seat) {
                continue;
            }
            // Domain [0, 4] (not [1, 4]) so the seat_trait can equal 0
            // for unused boats — H1 forces all x to 0, the link
            // equality forces seat_trait to 0, and a tighter [1, 4]
            // domain would make the whole problem infeasible whenever
            // any boat is left unused.
            let s_var = solver.new_bounded_integer(0, 4);
            let mut terms: Vec<AffineView<DomainId>> = Vec::new();
            for (r_idx, rower) in available.iter().enumerate() {
                if let Some(&var) = x.get(&(r_idx, b_idx, seat)) {
                    terms.push(var.scaled(ordinal(rower)));
                }
            }
            if terms.is_empty() {
                // No eligible rower for this seat — H1 will have
                // already forced use[b] = 0 for this boat, so skipping
                // is correct (the seat_trait var would be unused).
                continue;
            }
            terms.push(s_var.scaled(-1));
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(terms, 0, tag))
                .post()
                .map_err(|e| anyhow!("{label} (boat {b_idx}, seat {seat}): {e:?}"))?;
            map.insert((b_idx, seat), s_var);
        }
    }
    Ok(map)
}

fn seat_positions(boat: &Boat) -> Vec<i32> {
    let mut seats = Vec::with_capacity((boat.seat_count + 1) as usize);
    if boat.has_cox.as_bool() {
        seats.push(0);
    }
    for s in 1..=boat.seat_count {
        seats.push(s);
    }
    seats
}

/// Solver target ordinal for a boat weight class. Matches
/// [`RowerWeightClass::ordinal`] so the two can be compared directly in the
/// weight-class band constraint. `Tubby` clamps to `Heavy` — we don't have
/// a Tubby rower bucket, so the boat just acts as "as heavy as Heavy".
fn boat_target_weight_ordinal(wc: WeightClass) -> i32 {
    match wc {
        WeightClass::Light => 1,
        WeightClass::Medium => 2,
        WeightClass::Heavy => 3,
        WeightClass::Tubby => 3,
    }
}

fn rower_eligible_for_seat(rower: &Rower, boat: &Boat, seat: i32) -> bool {
    // Seat 0 is the cox seat: only rowers flagged `can_cox` are candidates,
    // and side is irrelevant for cox.
    if seat == 0 {
        return rower.can_cox.as_bool();
    }
    // Designated coxswains *only* cox — they never row a rowing seat,
    // regardless of side, weight class, or availability.
    if rower.is_designated_cox.as_bool() {
        return false;
    }
    // Rowing seats: the rower's side must match the seat's side, UNLESS
    // they're `Either` (matches anything) OR their side preference is
    // soft (`SideStrength` > 0), which makes wrong-side placement a
    // soft preference rather than a hard rule. `SideStrength::HARD` is
    // the hard-lock escape hatch — those rowers can only row their
    // preferred side.
    let seat_side = match boat.seat_side(seat) {
        Some(s) => s,
        None => return false, // out-of-range seat; shouldn't happen
    };
    match rower.side {
        Side::Either => true,
        r_side if r_side == seat_side => true,
        _ => !rower.side_strength.is_hard(),
    }
}

/// How many penalty points a (rower, boat, seat) placement contributes to
/// the S4 soft-side objective. Returns 0 for the cox seat, for `Either`
/// rowers, and for correct-side placements; otherwise returns the rower's
/// `side_strength` (which is guaranteed ≥ 1 here because the eligibility
/// filter already rejected `SideStrength::HARD` mismatches).
fn wrong_side_penalty(rower: &Rower, boat: &Boat, seat: i32) -> i32 {
    if seat == 0 {
        return 0;
    }
    let seat_side = match boat.seat_side(seat) {
        Some(s) => s,
        None => return 0,
    };
    if rower.side == Side::Either || rower.side == seat_side {
        return 0;
    }
    rower.side_strength.as_int()
}

fn decode_solution(
    x: &BTreeMap<(usize, usize, i32), DomainId>,
    use_b: &[DomainId],
    boats: &[&Boat],
    available: &[&Rower],
    mut value_of: impl FnMut(DomainId) -> i32,
) -> Vec<ProposedLineup> {
    let mut by_boat: BTreeMap<usize, Vec<(i32, usize)>> = BTreeMap::new();
    for (&(r_idx, b_idx, seat), &var) in x {
        if value_of(var) == 1 {
            by_boat.entry(b_idx).or_default().push((seat, r_idx));
        }
    }

    boats
        .iter()
        .enumerate()
        .map(|(b_idx, boat)| {
            let used = value_of(use_b[b_idx]) == 1;
            let mut seats: Vec<(i32, RowerId)> = by_boat
                .get(&b_idx)
                .map(|rows| {
                    rows.iter()
                        .map(|&(s, r_idx)| (s, available[r_idx].id))
                        .collect()
                })
                .unwrap_or_default();
            seats.sort_by_key(|&(s, _)| s);
            ProposedLineup {
                boat_id: boat.id,
                boat_name: boat.name.clone(),
                used,
                seats,
            }
        })
        .collect()
}
