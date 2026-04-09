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
use pumpkin_core::termination::Indefinite;
use pumpkin_core::variables::{DomainId, TransformableVariable};
use pumpkin_core::Solver;
use std::collections::BTreeMap;
use std::ops::ControlFlow;

#[derive(Debug, Clone)]
pub struct SolveRequest {
    pub date: NaiveDate,
    /// Boats to field today. The solver requires every seat in these boats
    /// to be filled. IDs must refer to entries in `snapshot.sweep_boats`.
    pub boats: Vec<BoatId>,
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
    /// (seat_position, rower_id). `seat_position = 0` is the cox seat on
    /// coxed boats; `1..=seat_count` are the rowing seats (bow → stroke).
    pub seats: Vec<(i32, RowerId)>,
}

/// Pick a reasonable set of boats to field today without human input. Used
/// by the CLI for its demo `solve` command; real requests will come from
/// the coach via the web UI in a later milestone.
///
/// We enumerate every subset of the available sweep fleet (cheap for a
/// club-sized fleet) and pick the subset that
///   1. has total seats ≤ `num_available_rowers`,
///   2. maximises total seats filled,
///   3. tie-breaks toward fielding *more* boats (more interesting for the
///      solver to demonstrate, and usually what coaches prefer).
pub fn greedy_fleet_selection(boats: &[Boat], num_available_rowers: usize) -> Vec<BoatId> {
    let n = boats.len();
    if n == 0 {
        return vec![];
    }
    let mut best_total: i32 = 0;
    let mut best_ids: Vec<BoatId> = vec![];
    for mask in 1u32..(1u32 << n) {
        let mut total = 0;
        let mut ids = Vec::new();
        for i in 0..n {
            if (mask >> i) & 1 == 1 {
                total += boat_seat_total(&boats[i]);
                ids.push(boats[i].id);
            }
        }
        if (total as usize) <= num_available_rowers
            && (total > best_total
                || (total == best_total && ids.len() > best_ids.len()))
        {
            best_total = total;
            best_ids = ids;
        }
    }
    best_ids
}

/// Find a feasible seat assignment for the requested boats. Hard constraints
/// only; no objective function.
#[tracing::instrument(level = "debug", skip_all, fields(date = %request.date, n_boats = request.boats.len()), err)]
pub fn solve(snapshot: &DbSnapshot, request: &SolveRequest) -> Result<SolveResult> {
    if request.boats.is_empty() {
        return Ok(SolveResult {
            status: SolveStatus::Satisfied,
            lineups: vec![],
        });
    }

    // Resolve boat IDs → &Boat from the snapshot.
    let boats: Vec<&Boat> = request
        .boats
        .iter()
        .map(|bid| {
            snapshot
                .sweep_boats
                .iter()
                .find(|b| b.id == *bid)
                .ok_or_else(|| anyhow!("boat {} not in snapshot sweep fleet", bid))
        })
        .collect::<Result<_>>()?;

    let available: Vec<&Rower> = snapshot.available_rowers().collect();

    if available.is_empty() {
        bail!("no rowers are available for sweep seating on {}", request.date);
    }

    let total_seats: i32 = boats.iter().map(|b| boat_seat_total(b)).sum();
    if (available.len() as i32) < total_seats {
        bail!(
            "not enough available rowers ({}) to fill requested boats ({} seats)",
            available.len(),
            total_seats
        );
    }

    let cox_capable = available.iter().filter(|r| r.can_cox.as_bool()).count();
    let coxed_boats = boats.iter().filter(|b| b.has_cox.as_bool()).count();
    if cox_capable < coxed_boats {
        bail!(
            "not enough cox-capable rowers ({}) for {} coxed boats",
            cox_capable,
            coxed_boats
        );
    }

    let mut solver = Solver::default();
    // x[(rower_idx, boat_idx, seat_position)] ∈ {0,1}
    let mut x: BTreeMap<(usize, usize, i32), DomainId> = BTreeMap::new();

    // --- Variables ---
    // A variable x[(r,b,s)] ∈ {0,1} is created only when rower r is eligible
    // for seat s of boat b — cox seat requires `can_cox`, rowing seats
    // require that the rower's side matches the seat's side (or that the
    // rower is `Side::Either`). Ineligible combinations don't exist in the
    // model at all, which is both faster and a little easier to read than
    // fixing their domain to {0}.
    for (b_idx, boat) in boats.iter().enumerate() {
        for seat in seat_positions(boat) {
            for (r_idx, rower) in available.iter().enumerate() {
                if !rower_eligible_for_seat(rower, boat, seat) {
                    continue;
                }
                let var = solver.new_bounded_integer(0, 1);
                x.insert((r_idx, b_idx, seat), var);
            }
        }
    }

    // --- Hard constraint 1: each seat is filled by exactly one rower. ---
    for (b_idx, boat) in boats.iter().enumerate() {
        for seat in seat_positions(boat) {
            let terms: Vec<DomainId> = (0..available.len())
                .filter_map(|r_idx| x.get(&(r_idx, b_idx, seat)).copied())
                .collect();
            if terms.is_empty() {
                bail!(
                    "no eligible rower for seat {} of boat {}",
                    seat,
                    boat.name
                );
            }
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(terms, 1, tag))
                .post()
                .map_err(|e| anyhow!("posting seat-fill constraint: {e:?}"))?;
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

    // --- H5 + S5: weight-class hard wall + soft target per boat ---
    //
    // The boat's weight class describes the rowers it's rigged for. A
    // badly-matched crew makes the boat sit wrong in the water: too light
    // and the hull rides high and becomes unstable in any chop; too heavy
    // and it sits low and drags.
    //
    // Encoding (see crates/solver/README.md §S5):
    //   1. A HARD WALL at `target_sum ± n_rowing` — prevents obviously-wrong
    //      crews (a full class of drift on average) without pushing the
    //      problem to the edge of infeasibility.
    //   2. A SOFT TARGET via slack variables: per boat,
    //           sum(ordinal * x) - over[b] + under[b] = target_sum
    //      where over[b], under[b] ≥ 0. At optimum only one is nonzero and
    //      together they equal `|sum - target_sum|`. Both are summed into
    //      the single objective variable and minimised below.
    //
    // The wall is two linear inequalities with positive / negated
    // coefficients. The slack equality is one linear equality per boat.
    let mut slack_vars: Vec<DomainId> = Vec::new();
    for (b_idx, boat) in boats.iter().enumerate() {
        let n_rowing = boat.seat_count;
        if n_rowing == 0 {
            continue;
        }
        let target = boat_target_weight_ordinal(boat.weight_class);
        let target_sum = target * n_rowing;
        let wall = n_rowing; // ±n_rowing ≈ one class of average drift

        // Rowing-seat variables for this boat, scaled by rower ordinal.
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

        // --- Hard wall: sum ≤ target_sum + wall ---
        let tag_hi = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::less_than_or_equals(
                positive_terms.clone(),
                target_sum + wall,
                tag_hi,
            ))
            .post()
            .map_err(|e| anyhow!("weight-class hard wall (upper): {e:?}"))?;

        let negative_terms: Vec<_> = x
            .iter()
            .filter_map(|(&(r_idx, b, seat), &var)| {
                if b != b_idx || seat == 0 {
                    return None;
                }
                Some(var.scaled(-available[r_idx].weight_class.ordinal()))
            })
            .collect();

        let tag_lo = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::less_than_or_equals(
                negative_terms,
                -(target_sum - wall),
                tag_lo,
            ))
            .post()
            .map_err(|e| anyhow!("weight-class hard wall (lower): {e:?}"))?;

        // --- Soft target slack variables ---
        // Worst case: sum hits the wall → |sum - target_sum| = wall = N.
        // 3*N gives us generous headroom for search to breathe.
        let slack_upper = 3 * n_rowing;
        let over = solver.new_bounded_integer(0, slack_upper);
        let under = solver.new_bounded_integer(0, slack_upper);

        // sum(ordinal * x) - over + under = target_sum
        let mut eq_terms: Vec<_> = positive_terms.clone();
        eq_terms.push(over.scaled(-1));
        eq_terms.push(under.scaled(1));
        let tag_eq = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::equals(eq_terms, target_sum, tag_eq))
            .post()
            .map_err(|e| anyhow!("weight-class slack equality: {e:?}"))?;

        slack_vars.push(over);
        slack_vars.push(under);
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

        let mut seat_skill_vars: Vec<DomainId> = Vec::with_capacity(n_rowing as usize);
        for seat in 1..=n_rowing {
            let s_var = solver.new_bounded_integer(1, 4);
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

        let boat_max = solver.new_bounded_integer(1, 4);
        let boat_min = solver.new_bounded_integer(1, 4);

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

        slack_vars.push(spread);
    }

    // --- Objective variable ---
    // The objective is the sum of all slack / penalty terms: S5 weight
    // deviation + S1 skill spread so far. Later soft constraints append
    // into the same `slack_vars` vec and ride the same minimisation.
    let objective_upper: i32 = slack_vars.len() as i32 * 3 * 8; // loose: 3N per slack, N ≤ 8
    let objective = solver.new_bounded_integer(0, objective_upper.max(1));
    if !slack_vars.is_empty() {
        // objective - sum(slacks) = 0
        let mut obj_terms: Vec<_> = slack_vars.iter().map(|v| v.scaled(1)).collect();
        obj_terms.push(objective.scaled(-1));
        let tag = solver.new_constraint_tag();
        solver
            .add_constraint(pumpkin_constraints::equals(obj_terms, 0, tag))
            .post()
            .map_err(|e| anyhow!("objective link: {e:?}"))?;
    }

    // --- Solve (optimisation) ---
    let mut brancher = solver.default_brancher();
    let mut resolver = ResolutionResolver::default();
    let mut termination = Indefinite;
    let procedure = LinearSatUnsat::new(OptimisationDirection::Minimise, objective, NoCallback);

    let result: SolveResult = match solver.optimise(
        &mut brancher,
        &mut termination,
        &mut resolver,
        procedure,
    ) {
        OptimisationResult::Optimal(sol) | OptimisationResult::Satisfiable(sol) => {
            // Both branches return a concrete `Solution` rather than a
            // reference, so we can decode without borrow gymnastics.
            let lineups =
                decode_solution(&x, &boats, &available, |v| sol.get_integer_value(v));
            SolveResult {
                status: SolveStatus::Satisfied,
                lineups,
            }
        }
        OptimisationResult::Stopped(sol, _) => {
            let lineups =
                decode_solution(&x, &boats, &available, |v| sol.get_integer_value(v));
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

fn boat_seat_total(boat: &Boat) -> i32 {
    boat.seat_count + if boat.has_cox.as_bool() { 1 } else { 0 }
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
    // regardless of side, weight class, or availability. Rejecting them
    // here keeps the constraint implicit in the eligibility filter rather
    // than requiring a separate posted constraint.
    if rower.is_designated_cox.as_bool() {
        return false;
    }
    // Rowing seats: the rower's side must match the seat's side, unless the
    // rower is `Either`. This is a HARD constraint regardless of
    // `side_strength` — the soft-preference path will arrive with the
    // objective function in a later milestone.
    let seat_side = match boat.seat_side(seat) {
        Some(s) => s,
        None => return false, // out-of-range seat; shouldn't happen
    };
    match rower.side {
        Side::Either => true,
        r_side => r_side == seat_side,
    }
}

fn decode_solution(
    x: &BTreeMap<(usize, usize, i32), DomainId>,
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
                seats,
            }
        })
        .collect()
}
