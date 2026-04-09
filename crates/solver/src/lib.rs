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
    /// Wall-clock budget the solver may spend looking for an optimal
    /// assignment. `None` lets the solver run to proven optimality
    /// (`Indefinite`), which is fine for small instances but can take
    /// minutes on a full club fleet. For interactive use, set this to
    /// a few seconds — the solver returns best-found-so-far with
    /// `SolveStatus::Timeout` if the budget expires before optimality
    /// is proven.
    pub time_budget: Option<std::time::Duration>,
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
    for (b_idx, boat) in boats.iter().enumerate() {
        let seats_total = boat.seat_count + if boat.has_cox.as_bool() { 1 } else { 0 };
        obj_terms.push(use_b[b_idx].scaled(-seats_total));
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
    for (&r_idx, wrong_vars) in &wrong_side_by_rower {
        if wrong_vars.is_empty() {
            continue;
        }
        let rower = available[r_idx];
        let wrong_count = solver.new_bounded_integer(0, 1);
        let mut link_terms: Vec<AffineView<DomainId>> =
            wrong_vars.iter().map(|v| v.scaled(1)).collect();
        link_terms.push(wrong_count.scaled(-1));
        let tag = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::equals(link_terms, 0, tag))
            .post()
            .map_err(|e| anyhow!("S4 wrong-side link: {e:?}"))?;

        obj_terms.push(wrong_count.scaled(rower.side_strength.as_int()));
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

        obj_terms.push(over.scaled(1));
        obj_terms.push(under.scaled(1));
    }

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
    // No weighting yet — each unit of skill spread weighs the same as one
    // unit of weight-class deviation. User tuning arrives when we
    // introduce SolverConfig.
    for (b_idx, boat) in boats.iter().enumerate() {
        let n_rowing = boat.seat_count;
        if n_rowing == 0 {
            continue;
        }

        // Skip OPTIONAL seats (partial-fill territory): an optional seat
        // may be empty even when the boat is used, so its `seat_skill`
        // would be 0 and pollute the max/min. Aggregating over required
        // seats only gives a correct signal for the "stable core" of
        // the boat, and matches the natural reading of skill variance
        // for partial-fill crews.
        let opt_seats = optional_seats(boat);
        let mut seat_skill_vars: Vec<DomainId> = Vec::with_capacity(n_rowing as usize);
        for seat in 1..=n_rowing {
            if opt_seats.contains(&seat) {
                continue;
            }
            // Domain [0, 4] (not [1, 4]) because the seat_skill has to
            // equal 0 for unused boats — H1 forces all x to 0, the link
            // equality forces seat_skill to 0, and a tighter [1, 4]
            // domain would make the whole problem infeasible whenever
            // any boat is left unused.
            let s_var = solver.new_bounded_integer(0, 4);
            let mut terms: Vec<_> = Vec::new();
            for (r_idx, rower) in available.iter().enumerate() {
                if let Some(&var) = x.get(&(r_idx, b_idx, seat)) {
                    terms.push(var.scaled(rower.skill.ordinal()));
                }
            }
            if terms.is_empty() {
                continue; // unreachable for requested boats — H1 would have bailed
            }
            terms.push(s_var.scaled(-1));
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(terms, 0, tag))
                .post()
                .map_err(|e| anyhow!("seat skill link (boat {b_idx}, seat {seat}): {e:?}"))?;
            seat_skill_vars.push(s_var);
        }

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

        obj_terms.push(spread.scaled(1));
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

                obj_terms.push(together.scaled(-aff.weight.as_int()));

                s_lo += 2;
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
                obj_terms.push(var.scaled(-aff.weight.as_int()));
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
    for (b_idx, boat) in boats.iter().enumerate() {
        let n_rowing = boat.seat_count;
        if n_rowing < 2 {
            continue;
        }

        // Skip OPTIONAL seats: under partial-fill policies these may be
        // empty and their seat_strength would be 0, polluting the pair
        // diff computation. Keyed by seat position so the partition
        // iteration below can check presence rather than relying on a
        // dense index.
        let opt_seats = optional_seats(boat);
        let mut seat_strength_by_seat: BTreeMap<i32, DomainId> = BTreeMap::new();
        for seat in 1..=n_rowing {
            if opt_seats.contains(&seat) {
                continue;
            }
            let s_var = solver.new_bounded_integer(0, 4);
            let mut terms: Vec<_> = Vec::new();
            for (r_idx, rower) in available.iter().enumerate() {
                if let Some(&var) = x.get(&(r_idx, b_idx, seat)) {
                    terms.push(var.scaled(rower.strength.ordinal()));
                }
            }
            if terms.is_empty() {
                // Unreachable for requested boats — H1 would have bailed.
                continue;
            }
            terms.push(s_var.scaled(-1));
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(terms, 0, tag))
                .post()
                .map_err(|e| anyhow!("seat strength link: {e:?}"))?;
            seat_strength_by_seat.insert(seat, s_var);
        }

        // Iterate 2-seat partitions and penalise strength spread. Skip
        // any partition that includes an optional seat — when the seat
        // is empty the pair-strength diff is meaningless, and when it's
        // filled we don't want a partial-fill-capable partition's
        // balance to dominate the objective either.
        let mut s_lo = 1i32;
        while s_lo + 1 <= n_rowing {
            let s_hi = s_lo + 1;
            let (lo_var, hi_var) = match (
                seat_strength_by_seat.get(&s_lo).copied(),
                seat_strength_by_seat.get(&s_hi).copied(),
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

            obj_terms.push(diff.scaled(1));

            s_lo += 2;
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
