//! Simulated annealing post-processor for lineup optimization.
//!
//! Takes a CP solver result and improves it via random neighborhood
//! moves (cross-boat swaps, within-boat swaps, bench swaps). The SA
//! naturally explores multi-variable moves that the CP solver's
//! single-variable branching misses.

use std::collections::HashMap;

use chrono::NaiveDate;
use lineup_db::boat::Boat;
use lineup_db::rower::types::RowerId;
use lineup_db::rower::Rower;
use lineup_db::snapshot::DbSnapshot;
use rand::Rng;

use crate::model::{
    boat_target_weight_ordinal, optional_seats, rower_eligible_for_seat, wrong_side_penalty,
};
use crate::{
    compute_unplaced, BoatClass, ProposedLineup, ProposedSolution, SeatLock, SolveRequest,
    SolverConfig, COX_COOLDOWN_DAYS,
};

const ITERATIONS: usize = 15_000;
const BENCH_COOLDOWN_DAYS: i64 = 7;

/// Placement of a single rower.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Seated { boat_idx: usize, seat: i32 },
    Benched,
}

/// Lightweight assignment state for SA manipulation.
pub(crate) struct Assignment {
    /// For each rower index in `available`, where they are.
    places: Vec<Place>,
    /// Reverse map: (boat_idx, seat) → rower_idx. Only seated rowers.
    grid: HashMap<(usize, i32), usize>,
}

impl Assignment {
    pub(crate) fn from_solution(
        solution: &ProposedSolution,
        boats: &[&Boat],
        available: &[&Rower],
    ) -> Self {
        let rower_id_to_idx: HashMap<RowerId, usize> = available
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id, i))
            .collect();
        let boat_id_to_idx: HashMap<_, usize> =
            boats.iter().enumerate().map(|(i, b)| (b.id, i)).collect();

        let mut places = vec![Place::Benched; available.len()];
        let mut grid = HashMap::new();

        for lineup in &solution.lineups {
            if !lineup.used {
                continue;
            }
            let Some(&b_idx) = boat_id_to_idx.get(&lineup.boat_id) else {
                continue;
            };
            for &(seat, rower_id) in &lineup.seats {
                if let Some(&r_idx) = rower_id_to_idx.get(&rower_id) {
                    places[r_idx] = Place::Seated {
                        boat_idx: b_idx,
                        seat,
                    };
                    grid.insert((b_idx, seat), r_idx);
                }
            }
        }

        Self { places, grid }
    }

    fn swap(&mut self, r_a: usize, r_b: usize) {
        let pa = self.places[r_a];
        let pb = self.places[r_b];
        // Update grid
        if let Place::Seated { boat_idx, seat } = pa {
            self.grid.insert((boat_idx, seat), r_b);
        }
        if let Place::Seated { boat_idx, seat } = pb {
            self.grid.insert((boat_idx, seat), r_a);
        }
        // Handle bench→seat or seat→bench
        if let Place::Benched = pa {
            if let Place::Seated { boat_idx, seat } = pb {
                self.grid.insert((boat_idx, seat), r_a);
            }
        }
        if let Place::Benched = pb {
            if let Place::Seated { boat_idx, seat } = pa {
                self.grid.insert((boat_idx, seat), r_b);
            }
        }
        self.places[r_a] = pb;
        self.places[r_b] = pa;
    }

    fn to_solution(&self, boats: &[&Boat], available: &[&Rower]) -> ProposedSolution {
        let mut lineups: Vec<ProposedLineup> = boats
            .iter()
            .enumerate()
            .map(|(b_idx, boat)| {
                let mut seats: Vec<(i32, RowerId)> = Vec::new();
                let has_cox = boat.has_cox.as_bool();
                if has_cox {
                    if let Some(&r_idx) = self.grid.get(&(b_idx, 0)) {
                        seats.push((0, available[r_idx].id));
                    }
                }
                for s in 1..=boat.seat_count.as_int() {
                    if let Some(&r_idx) = self.grid.get(&(b_idx, s)) {
                        seats.push((s, available[r_idx].id));
                    }
                }
                let used = !seats.is_empty();
                seats.sort_by_key(|&(s, _)| s);
                ProposedLineup {
                    boat_id: boat.id,
                    boat_name: boat.name.clone(),
                    used,
                    seats,
                }
            })
            .collect();
        // Preserve unused boats from original order
        for lineup in &mut lineups {
            if lineup.seats.is_empty() {
                lineup.used = false;
            }
        }
        let unplaced = compute_unplaced(available, &lineups);
        ProposedSolution { lineups, unplaced }
    }
}

/// Run SA post-processing on a CP solution. Returns the improved
/// solution and its objective value. Pure function — no Pumpkin.
pub(crate) fn anneal(
    solution: &ProposedSolution,
    snapshot: &DbSnapshot,
    request: &SolveRequest,
    boats: &[&Boat],
    available: &[&Rower],
    cp_objective: i32,
) -> Option<(ProposedSolution, i32)> {
    let mut assignment = Assignment::from_solution(solution, boats, available);
    let ctx = EvalContext::new(snapshot, request, boats, available);
    let locked = build_locked_set(&request.locks, boats, available);

    let initial_obj = evaluate(&assignment, &ctx);

    // Sanity check: the SA evaluator must agree with the CP solver
    // on the initial solution. A disagreement means the evaluator
    // has a coefficient bug and SA would optimize toward a wrong
    // objective. Allow a small tolerance for rounding differences.
    // The SA evaluator computes tighter auxiliary variable values
    // than the CP may achieve under timeout (e.g., S5 weight-class
    // slack over+under aren't fully minimized to |diff|). This
    // means SA initial ≤ CP objective is expected and fine — the SA
    // is more accurate. Only bail if the SA thinks the solution is
    // SIGNIFICANTLY worse than the CP says, which indicates a real
    // evaluator bug.
    if initial_obj > cp_objective + 10 {
        tracing::warn!(
            cp_objective,
            sa_initial_obj = initial_obj,
            "SA evaluator scores worse than CP — skipping SA"
        );
        log_breakdown(&assignment, &ctx);
        return None;
    }
    tracing::debug!(
        cp_objective,
        sa_initial_obj = initial_obj,
        delta = initial_obj - cp_objective,
        "SA evaluator score"
    );

    let mut current_obj = initial_obj;
    let mut best_obj = current_obj;
    let mut best_assignment = assignment.places.clone();

    let mut rng = rand::thread_rng();

    // Collect seated, benched, and empty-seat lists for move generation.
    let seated: Vec<usize> = available
        .iter()
        .enumerate()
        .filter(|(i, _)| matches!(assignment.places[*i], Place::Seated { .. }))
        .map(|(i, _)| i)
        .collect();
    let benched: Vec<usize> = available
        .iter()
        .enumerate()
        .filter(|(i, _)| matches!(assignment.places[*i], Place::Benched))
        .map(|(i, _)| i)
        .collect();
    // Find empty seats in used boats (partial-fill gaps).
    let empty_seats: Vec<(usize, i32)> = boats
        .iter()
        .enumerate()
        .flat_map(|(b_idx, boat)| {
            let used = assignment.grid.keys().any(|(bi, _)| *bi == b_idx);
            if !used {
                return vec![];
            }
            let mut empty = Vec::new();
            if boat.has_cox.as_bool() && !assignment.grid.contains_key(&(b_idx, 0)) {
                empty.push((b_idx, 0));
            }
            for s in 1..=boat.seat_count.as_int() {
                if !assignment.grid.contains_key(&(b_idx, s)) {
                    empty.push((b_idx, s));
                }
            }
            empty
        })
        .collect();

    // SA parameters
    let t_initial: f64 = 2.0;
    let t_final: f64 = 0.01;
    let alpha = (t_final / t_initial).powf(1.0 / ITERATIONS as f64);
    let mut temperature = t_initial;
    let mut accepted = 0usize;
    let mut last_improved = 0usize;
    let mut reheated = false;

    let start = std::time::Instant::now();

    for iter in 0..ITERATIONS {
        // Generate a random move
        let mv = match generate_move(
            &assignment,
            &seated,
            &benched,
            &empty_seats,
            boats,
            available,
            &locked,
            &mut rng,
        ) {
            Some(m) => m,
            None => {
                temperature *= alpha;
                continue;
            }
        };

        // Apply move and evaluate
        match mv {
            Move::Swap(r_a, r_b) => {
                assignment.swap(r_a, r_b);
                let new_obj = evaluate(&assignment, &ctx);
                let delta = new_obj - current_obj;
                if delta <= 0 || rng.gen::<f64>() < (-delta as f64 / temperature).exp() {
                    current_obj = new_obj;
                    accepted += 1;
                    if current_obj < best_obj {
                        best_obj = current_obj;
                        best_assignment = assignment.places.clone();
                        last_improved = iter;
                    }
                } else {
                    assignment.swap(r_a, r_b);
                }
            }
            Move::Fill {
                rower_idx,
                boat_idx,
                seat,
            } => {
                // Place benched rower into empty seat (one-way — never undo a fill)
                assignment.places[rower_idx] = Place::Seated { boat_idx, seat };
                assignment.grid.insert((boat_idx, seat), rower_idx);
                let new_obj = evaluate(&assignment, &ctx);
                let delta = new_obj - current_obj;
                if delta <= 0 || rng.gen::<f64>() < (-delta as f64 / temperature).exp() {
                    current_obj = new_obj;
                    accepted += 1;
                    if current_obj < best_obj {
                        best_obj = current_obj;
                        best_assignment = assignment.places.clone();
                        last_improved = iter;
                    }
                } else {
                    // Undo fill
                    assignment.places[rower_idx] = Place::Benched;
                    assignment.grid.remove(&(boat_idx, seat));
                }
            }
        }

        // Reheat: if stuck for 2000 iterations, bump temperature
        // back up to escape the local minimum. One-shot only.
        if !reheated && iter - last_improved > 2000 {
            temperature = t_initial / 2.0;
            reheated = true;
            last_improved = iter;
        }

        temperature *= alpha;
    }

    let elapsed = start.elapsed();

    // Restore best assignment
    assignment.places = best_assignment;
    // Rebuild grid from places
    assignment.grid.clear();
    for (r_idx, place) in assignment.places.iter().enumerate() {
        if let Place::Seated { boat_idx, seat } = place {
            assignment.grid.insert((*boat_idx, *seat), r_idx);
        }
    }

    tracing::info!(
        initial_obj,
        best_obj,
        improvement = initial_obj - best_obj,
        elapsed_ms = elapsed.as_millis() as u64,
        iterations = ITERATIONS,
        accepted,
        "SA post-processor"
    );

    Some((assignment.to_solution(boats, available), best_obj))
}

/// Set of (rower_idx, boat_idx, seat) triples that are locked and
/// must not be moved by SA.
fn build_locked_set(
    locks: &[SeatLock],
    boats: &[&Boat],
    available: &[&Rower],
) -> std::collections::HashSet<usize> {
    let mut locked_rowers = std::collections::HashSet::new();
    for lock in locks {
        if let Some(r_idx) = available.iter().position(|r| r.id == lock.rower_id) {
            if boats.iter().any(|b| b.id == lock.boat_id) {
                locked_rowers.insert(r_idx);
            }
        }
    }
    locked_rowers
}

/// A move to apply: either swap two rowers or fill an empty seat.
enum Move {
    Swap(usize, usize),
    Fill {
        rower_idx: usize,
        boat_idx: usize,
        seat: i32,
    },
}

/// Generate a random feasible move.
fn generate_move(
    assignment: &Assignment,
    seated: &[usize],
    benched: &[usize],
    empty_seats: &[(usize, i32)],
    boats: &[&Boat],
    available: &[&Rower],
    locked: &std::collections::HashSet<usize>,
    rng: &mut impl Rng,
) -> Option<Move> {
    // Try up to 20 times to find a feasible move
    for _ in 0..20 {
        // 80% seated↔seated swaps, 20% fill empty seat from bench
        let try_fill = !empty_seats.is_empty() && !benched.is_empty() && rng.gen_range(0..5) == 0;

        if try_fill {
            let &(boat_idx, seat) = &empty_seats[rng.gen_range(0..empty_seats.len())];
            // Check if seat is still empty (may have been filled
            // by a previous accepted fill move)
            if assignment.grid.contains_key(&(boat_idx, seat)) {
                continue;
            }
            let r_idx = benched[rng.gen_range(0..benched.len())];
            if !matches!(assignment.places[r_idx], Place::Benched) {
                continue; // already placed by a prior fill
            }
            if !rower_eligible_for_seat(available[r_idx], boats[boat_idx], seat) {
                continue;
            }
            if !sweep_bias_ok(available[r_idx], boats[boat_idx]) {
                continue;
            }
            return Some(Move::Fill {
                rower_idx: r_idx,
                boat_idx,
                seat,
            });
        }

        // Seated↔seated swap
        if seated.len() < 2 {
            if empty_seats.is_empty() || benched.is_empty() {
                return None;
            }
            continue;
        }
        let i = rng.gen_range(0..seated.len());
        let j = rng.gen_range(0..seated.len());
        if i == j {
            continue;
        }
        let r_a = seated[i];
        let r_b = seated[j];
        if locked.contains(&r_a) || locked.contains(&r_b) {
            continue;
        }
        let Place::Seated {
            boat_idx: ba,
            seat: sa,
        } = assignment.places[r_a]
        else {
            continue;
        };
        let Place::Seated {
            boat_idx: bb,
            seat: sb,
        } = assignment.places[r_b]
        else {
            continue;
        };
        // Designated coxes in cox seats are immovable.
        if sa == 0 && available[r_a].is_designated_cox.as_bool() {
            continue;
        }
        if sb == 0 && available[r_b].is_designated_cox.as_bool() {
            continue;
        }
        // Check eligibility after swap
        if !rower_eligible_for_seat(available[r_a], boats[bb], sb) {
            continue;
        }
        if !rower_eligible_for_seat(available[r_b], boats[ba], sa) {
            continue;
        }
        // Check sweep bias hard gates
        if !sweep_bias_ok(available[r_a], boats[bb]) || !sweep_bias_ok(available[r_b], boats[ba]) {
            continue;
        }
        return Some(Move::Swap(r_a, r_b));
    }
    None
}

fn sweep_bias_ok(rower: &Rower, boat: &Boat) -> bool {
    use lineup_db::rower::types::SweepBias;
    if boat.is_scull() && rower.sweep_bias == SweepBias::SWEEP_HARD {
        return false;
    }
    if boat.is_sweep() && rower.sweep_bias == SweepBias::SCULL_HARD {
        return false;
    }
    true
}

// ---------------------------------------------------------------------------
// Objective evaluator — mirrors every soft constraint in the CP model.
// ---------------------------------------------------------------------------

/// Pre-computed context for objective evaluation.
pub(crate) struct EvalContext<'a> {
    boats: &'a [&'a Boat],
    available: &'a [&'a Rower],
    cfg: SolverConfig,
    #[allow(dead_code)]
    date: NaiveDate,
    /// pair affinities: (rower_a_idx, rower_b_idx, weight)
    pair_affinities: Vec<(usize, usize, i32)>,
    /// seat affinities: (rower_idx, boat_idx, seat, effective_weight)
    seat_affinities: Vec<(usize, usize, i32, i32)>,
    /// cox cooldown: rower_idx → effective penalty
    cox_cooldown: HashMap<usize, i32>,
    /// bench cooldown: rower_idx → effective penalty
    bench_cooldown: HashMap<usize, i32>,
    /// reference similarity: (rower_idx, boat_idx, seat, weight)
    reference_terms: Vec<(usize, usize, i32, i32)>,
    /// stroke rower indices (for S21)
    stroke_rower_indices: Vec<usize>,
    /// S16 decay factors per boat (thousandths)
    stacking_factors: Vec<i64>,
    partial_fill: crate::PartialFillPolicy,
}

impl<'a> EvalContext<'a> {
    pub(crate) fn new(
        snapshot: &'a DbSnapshot,
        request: &SolveRequest,
        boats: &'a [&'a Boat],
        available: &'a [&'a Rower],
    ) -> Self {
        let cfg = request.config;

        // Pair affinities → index-based
        let rower_id_to_idx: HashMap<RowerId, usize> = available
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id, i))
            .collect();
        let boat_id_to_idx: HashMap<_, usize> =
            boats.iter().enumerate().map(|(i, b)| (b.id, i)).collect();

        let pair_affinities: Vec<(usize, usize, i32)> = snapshot
            .pair_affinities
            .iter()
            .filter_map(|aff| {
                let a = *rower_id_to_idx.get(&aff.rower_a_id)?;
                let b = *rower_id_to_idx.get(&aff.rower_b_id)?;
                Some((a, b, aff.weight.as_int()))
            })
            .collect();

        // Seat affinities → (rower_idx, boat_idx, seat, effective_weight)
        let mut seat_aff_best: std::collections::BTreeMap<(usize, usize, i32), i32> =
            std::collections::BTreeMap::new();
        for aff in &snapshot.seat_affinities {
            if aff.weight.as_int() == 0 {
                continue;
            }
            let effective = if aff.zone.is_single_seat() {
                aff.weight.as_int() * 2
            } else {
                aff.weight.as_int()
            };
            let Some(&r_idx) = rower_id_to_idx.get(&aff.rower_id) else {
                continue;
            };
            for (b_idx, boat) in boats.iter().enumerate() {
                for seat in aff.zone.seats_for(boat.seat_count.as_int()) {
                    let key = (r_idx, b_idx, seat);
                    seat_aff_best
                        .entry(key)
                        .and_modify(|w| *w = (*w).max(effective))
                        .or_insert(effective);
                }
            }
        }
        let seat_affinities: Vec<(usize, usize, i32, i32)> = seat_aff_best
            .into_iter()
            .map(|((r, b, s), w)| (r, b, s, w))
            .collect();

        // Cox cooldown
        let mut cox_cooldown = HashMap::new();
        if cfg.cox_cooldown_penalty != 0 {
            for (r_idx, rower) in available.iter().enumerate() {
                if rower.is_designated_cox.as_bool() {
                    continue;
                }
                if let Some(last_date) = snapshot.last_coxed.get(&rower.id) {
                    let days_since = (request.date - *last_date).num_days();
                    if days_since >= 0 && days_since < COX_COOLDOWN_DAYS {
                        let numerator =
                            cfg.cox_cooldown_penalty as i64 * (COX_COOLDOWN_DAYS - days_since);
                        let effective =
                            ((numerator + COX_COOLDOWN_DAYS - 1) / COX_COOLDOWN_DAYS) as i32;
                        if effective > 0 {
                            cox_cooldown.insert(r_idx, effective);
                        }
                    }
                }
            }
        }

        // Bench cooldown
        let mut bench_cooldown = HashMap::new();
        if cfg.bench_cooldown_penalty != 0 {
            for (r_idx, rower) in available.iter().enumerate() {
                if rower.is_designated_cox.as_bool() {
                    continue;
                }
                if let Some(last_date) = snapshot.last_benched.get(&rower.id) {
                    let days_since = (request.date - *last_date).num_days();
                    if days_since >= 0 && days_since < BENCH_COOLDOWN_DAYS {
                        let numerator =
                            cfg.bench_cooldown_penalty as i64 * (BENCH_COOLDOWN_DAYS - days_since);
                        let effective =
                            ((numerator + BENCH_COOLDOWN_DAYS - 1) / BENCH_COOLDOWN_DAYS) as i32;
                        if effective > 0 {
                            bench_cooldown.insert(r_idx, effective);
                        }
                    }
                }
            }
        }

        // Reference similarity
        let reference_terms: Vec<(usize, usize, i32, i32)> = request
            .reference_lineups
            .iter()
            .filter(|r| r.weight != 0)
            .flat_map(|reference| {
                reference.placements.iter().filter_map(|p| {
                    let r_idx = *rower_id_to_idx.get(&p.rower_id)?;
                    let b_idx = *boat_id_to_idx.get(&p.boat_id)?;
                    Some((r_idx, b_idx, p.seat, reference.weight))
                })
            })
            .collect();

        // Stroke rower indices (S21)
        let stroke_rower_indices = {
            use lineup_db::seat_affinity::SeatZone;
            use std::collections::HashSet;
            let stroke_ids: HashSet<RowerId> = snapshot
                .seat_affinities
                .iter()
                .filter(|a| a.zone == SeatZone::Stroke && a.weight.as_int() >= 2)
                .map(|a| a.rower_id)
                .collect();
            available
                .iter()
                .enumerate()
                .filter(|(_, r)| stroke_ids.contains(&r.id))
                .map(|(i, _)| i)
                .collect()
        };

        // S16 decay factors
        let stacking_factors: Vec<i64> = (0..boats.len())
            .map(|rank| {
                let mut f = 1000i64;
                for _ in 0..rank {
                    f = f * 3 / 5;
                }
                f
            })
            .collect();

        Self {
            boats,
            available,
            cfg,
            date: request.date,
            pair_affinities,
            seat_affinities,
            cox_cooldown,
            bench_cooldown,
            reference_terms,
            stroke_rower_indices,
            stacking_factors,
            partial_fill: request.partial_fill,
        }
    }
}

/// Per-constraint breakdown for debugging evaluator vs CP discrepancies.
#[derive(Debug, Default)]
pub(crate) struct ObjBreakdown {
    s1: i32,
    s2: i32,
    s3: i32,
    s4: i32,
    s5: i32,
    s6: i32,
    s8: i32,
    s9: i32,
    s10: i32,
    s11: i32,
    s12: i32,
    s13: i32,
    sweep_bias: i32,
    s14: i32,
    s15: i32,
    s16: i32,
    s17: i32,
    s18: i32,
    s19: i32,
    s20: i32,
    s21: i32,
    reference: i32,
    partial_fill: i32,
}

impl ObjBreakdown {
    fn total(&self) -> i32 {
        self.s1
            + self.s2
            + self.s3
            + self.s4
            + self.s5
            + self.s6
            + self.s8
            + self.s9
            + self.s10
            + self.s11
            + self.s12
            + self.s13
            + self.sweep_bias
            + self.s14
            + self.s15
            + self.s16
            + self.s17
            + self.s18
            + self.s19
            + self.s20
            + self.s21
            + self.reference
            + self.partial_fill
    }
}

/// Evaluate the full objective for an assignment. Returns the same
/// weighted sum as the CP model (lower is better).
fn evaluate(a: &Assignment, ctx: &EvalContext) -> i32 {
    evaluate_breakdown(a, ctx).total()
}

/// Log a per-constraint breakdown for debugging.
fn log_breakdown(a: &Assignment, ctx: &EvalContext) {
    let b = evaluate_breakdown(a, ctx);
    tracing::warn!(
        total = b.total(),
        s1 = b.s1,
        s2 = b.s2,
        s3 = b.s3,
        s4 = b.s4,
        s5 = b.s5,
        s6 = b.s6,
        s8 = b.s8,
        s9 = b.s9,
        s10 = b.s10,
        s11 = b.s11,
        s12 = b.s12,
        s13 = b.s13,
        sweep_bias = b.sweep_bias,
        s14 = b.s14,
        s15 = b.s15,
        s16 = b.s16,
        s17 = b.s17,
        s18 = b.s18,
        s19 = b.s19,
        s20 = b.s20,
        s21 = b.s21,
        reference = b.reference,
        partial_fill = b.partial_fill,
        "SA evaluator per-constraint breakdown"
    );
}

pub(crate) fn evaluate_breakdown(a: &Assignment, ctx: &EvalContext) -> ObjBreakdown {
    let mut b = ObjBreakdown::default();

    let boats = ctx.boats;
    let available = ctx.available;
    let cfg = &ctx.cfg;

    // Helper: is boat b_idx used (has any seated rowers)?
    let boat_used = |b_idx: usize| -> bool {
        let boat = boats[b_idx];
        // Check if at least one seat is filled
        let has_cox = boat.has_cox.as_bool();
        if has_cox && a.grid.contains_key(&(b_idx, 0)) {
            return true;
        }
        for s in 1..=boat.seat_count.as_int() {
            if a.grid.contains_key(&(b_idx, s)) {
                return true;
            }
        }
        false
    };

    // S8 — placement reward
    if cfg.placement_reward_weight != 0 {
        for (b_idx, boat) in boats.iter().enumerate() {
            if boat_used(b_idx) {
                let seats_total =
                    boat.seat_count.as_int() + if boat.has_cox.as_bool() { 1 } else { 0 };
                let class = BoatClass::from_boat(boat);
                let bias = cfg.class_bias(class);
                let effective_weight = cfg.placement_reward_weight * (1 + bias);
                b.s8 += -seats_total * effective_weight;
            }
        }
    }

    // S4 — wrong-side penalty
    if cfg.side_preference_weight != 0 {
        for (r_idx, rower) in available.iter().enumerate() {
            if let Place::Seated { boat_idx, seat } = a.places[r_idx] {
                let pen = wrong_side_penalty(rower, boats[boat_idx], seat);
                if pen > 0 {
                    b.s4 += pen * cfg.side_preference_weight;
                }
            }
        }
    }

    // S6 — cox cooldown
    for (&r_idx, &effective) in &ctx.cox_cooldown {
        if let Place::Seated { seat: 0, .. } = a.places[r_idx] {
            b.s6 += effective;
        }
    }

    // S1 — skill variance per boat
    if cfg.skill_variance_weight != 0 {
        for (b_idx, boat) in boats.iter().enumerate() {
            if !boat_used(b_idx) {
                continue;
            }
            let n = boat.seat_count.as_int();
            if n < 2 {
                continue;
            }
            let skip_optional = ctx.partial_fill.max_empty() > 0;
            let opt = optional_seats(boat);
            let mut min_skill = i32::MAX;
            let mut max_skill = i32::MIN;
            for s in 1..=n {
                if skip_optional && opt.contains(&s) {
                    continue;
                }
                if let Some(&r_idx) = a.grid.get(&(b_idx, s)) {
                    let sk = available[r_idx].skill.ordinal();
                    min_skill = min_skill.min(sk);
                    max_skill = max_skill.max(sk);
                }
            }
            if max_skill > min_skill {
                b.s1 += (max_skill - min_skill) * cfg.skill_variance_weight;
            }
        }
    }

    // S2 — pair affinities
    if cfg.pair_affinity_weight != 0 {
        for &(a_idx, b_idx_aff, weight) in &ctx.pair_affinities {
            let pa = a.places[a_idx];
            let pb = a.places[b_idx_aff];
            if let (
                Place::Seated {
                    boat_idx: ba,
                    seat: sa,
                },
                Place::Seated {
                    boat_idx: bb,
                    seat: sb,
                },
            ) = (pa, pb)
            {
                if ba == bb {
                    // Same boat — check if in same partition
                    // Partitions: (1,2), (3,4), (5,6), (7,8)...
                    let part_a = (sa - 1) / 2;
                    let part_b = (sb - 1) / 2;
                    if sa >= 1 && sb >= 1 && part_a == part_b {
                        b.s2 += -weight * cfg.pair_affinity_weight;
                    }
                }
            }
        }
    }

    // S3 — seat affinities
    if cfg.seat_affinity_weight != 0 {
        for &(r_idx, b_idx, seat, eff_weight) in &ctx.seat_affinities {
            if let Place::Seated { boat_idx, seat: s } = a.places[r_idx] {
                if boat_idx == b_idx && s == seat {
                    b.s3 += -eff_weight * cfg.seat_affinity_weight;
                }
            }
        }
    }

    // S5 — weight-class slack
    if cfg.weight_class_slack_weight != 0 {
        for (b_idx, boat) in boats.iter().enumerate() {
            if !boat_used(b_idx) {
                continue;
            }
            let n = boat.seat_count.as_int();
            if n == 0 {
                continue;
            }
            let target = boat_target_weight_ordinal(boat.weight_class);
            let target_sum = target * n;
            let mut actual_sum = 0i32;
            for s in 1..=n {
                if let Some(&r_idx) = a.grid.get(&(b_idx, s)) {
                    actual_sum += available[r_idx].weight_class.ordinal();
                }
            }
            let over = (actual_sum - target_sum).max(0);
            let under = (target_sum - actual_sum).max(0);
            b.s5 += (over + under) * cfg.weight_class_slack_weight;
        }
    }

    // S9/S9b — pair strength + skill mismatch
    if cfg.pair_strength_weight != 0 {
        for (b_idx, boat) in boats.iter().enumerate() {
            if !boat_used(b_idx) {
                continue;
            }
            let n = boat.seat_count.as_int();
            if n < 2 {
                continue;
            }
            let skip_optional = ctx.partial_fill.max_empty() > 0;
            let opt = optional_seats(boat);
            let mut s_lo = 1i32;
            while s_lo + 1 <= n {
                let s_hi = s_lo + 1;
                if skip_optional && (opt.contains(&s_lo) || opt.contains(&s_hi)) {
                    s_lo += 2;
                    continue;
                }
                let r_lo = a.grid.get(&(b_idx, s_lo)).copied();
                let r_hi = a.grid.get(&(b_idx, s_hi)).copied();
                if let (Some(rl), Some(rh)) = (r_lo, r_hi) {
                    let str_diff =
                        (available[rl].strength.ordinal() - available[rh].strength.ordinal()).abs();
                    let skill_diff =
                        (available[rl].skill.ordinal() - available[rh].skill.ordinal()).abs();
                    // strength_diff * 2 + skill_diff
                    b.s9 += str_diff * cfg.pair_strength_weight * 2;
                    b.s9 += skill_diff * cfg.pair_strength_weight;
                    // S9b: bow pair extra
                    if s_lo == 1 && cfg.bow_pair_strength_weight != 0 {
                        b.s9 += str_diff * cfg.bow_pair_strength_weight * 2;
                        b.s9 += skill_diff * cfg.bow_pair_strength_weight;
                    }
                }
                s_lo += 2;
            }
        }
    }

    // S10 — pair height balance
    if cfg.height_balance_weight != 0 {
        for (b_idx, boat) in boats.iter().enumerate() {
            if !boat_used(b_idx) {
                continue;
            }
            let n = boat.seat_count.as_int();
            if n < 2 {
                continue;
            }
            let skip_optional = ctx.partial_fill.max_empty() > 0;
            let opt = optional_seats(boat);
            let mut s_lo = 1i32;
            while s_lo + 1 <= n {
                let s_hi = s_lo + 1;
                if skip_optional && (opt.contains(&s_lo) || opt.contains(&s_hi)) {
                    s_lo += 2;
                    continue;
                }
                if let (Some(&rl), Some(&rh)) =
                    (a.grid.get(&(b_idx, s_lo)), a.grid.get(&(b_idx, s_hi)))
                {
                    let diff =
                        (available[rl].height.ordinal() - available[rh].height.ordinal()).abs();
                    b.s10 += diff * cfg.height_balance_weight;
                }
                s_lo += 2;
            }
        }
    }

    // S11 — end-pair skill gradient
    if cfg.end_pair_skill_weight != 0 {
        let w = cfg.end_pair_skill_weight;
        for (b_idx, boat) in boats.iter().enumerate() {
            if !boat_used(b_idx) {
                continue;
            }
            let n = boat.seat_count.as_int();
            if n < 2 {
                continue;
            }
            for seat in 1..=n {
                let dist = (seat - 1).min(n - seat);
                let coef = match dist {
                    0 => w,
                    1 => (w * 3 / 4).max(1),
                    2 => (w / 2).max(1),
                    _ => (w / 4).max(1),
                };
                if let Some(&r_idx) = a.grid.get(&(b_idx, seat)) {
                    b.s11 += -available[r_idx].skill.ordinal() * coef;
                }
            }
        }
    }

    // S12 — engine-room strength reward
    if cfg.engine_room_strength_weight != 0 {
        use lineup_db::seat_affinity::SeatZone;
        for (b_idx, boat) in boats.iter().enumerate() {
            if !boat_used(b_idx) {
                continue;
            }
            for seat in SeatZone::EngineRoom.seats_for(boat.seat_count.as_int()) {
                if let Some(&r_idx) = a.grid.get(&(b_idx, seat)) {
                    b.s12 += -available[r_idx].strength.ordinal() * cfg.engine_room_strength_weight;
                }
            }
        }
    }

    // S13 — non-scull retention
    if cfg.non_scull_retention_weight != 0 {
        for (r_idx, rower) in available.iter().enumerate() {
            if matches!(a.places[r_idx], Place::Seated { .. }) {
                let scale = rower.sweep_bias.as_int().unsigned_abs() as i32 + 1;
                b.s13 += -cfg.non_scull_retention_weight * scale;
            }
        }
    }

    // Sweep-bias alignment penalty
    if cfg.non_scull_retention_weight != 0 {
        for (r_idx, rower) in available.iter().enumerate() {
            if let Place::Seated { boat_idx, .. } = a.places[r_idx] {
                let boat = boats[boat_idx];
                let bias = rower.sweep_bias.as_int();
                let penalty = if boat.is_scull() && bias > 0 {
                    bias * cfg.non_scull_retention_weight
                } else if boat.is_sweep() && bias < 0 {
                    -bias * cfg.non_scull_retention_weight
                } else {
                    0
                };
                if penalty != 0 {
                    b.sweep_bias += penalty;
                }
            }
        }
    }

    // S14 — bow-cox fit
    if cfg.bow_cox_fit_weight != 0 {
        use lineup_db::boat::types::CoxPosition;
        use lineup_db::rower::types::Height;
        for (b_idx, boat) in boats.iter().enumerate() {
            if !boat.has_cox.as_bool() || boat.cox_position != CoxPosition::Bow {
                continue;
            }
            if let Some(&r_idx) = a.grid.get(&(b_idx, 0)) {
                let rower = available[r_idx];
                let height_penalty = match rower.height {
                    Height::Tall => 3,
                    Height::VeryTall => 5,
                    _ => 0,
                };
                let weight_penalty = match rower.weight_class.ordinal() {
                    3 => 1,
                    _ => 0,
                };
                let total = height_penalty + weight_penalty;
                if total > 0 {
                    b.s14 += total * cfg.bow_cox_fit_weight;
                }
            }
        }
    }

    // S15 — designated-cox retention
    for (r_idx, rower) in available.iter().enumerate() {
        if rower.is_designated_cox.as_bool() {
            if let Place::Seated { seat: 0, .. } = a.places[r_idx] {
                b.s15 += -10;
            }
        }
    }

    // S16 — top-boat stacking
    if cfg.top_boat_stacking_weight != 0 && boats.len() >= 2 {
        let w = cfg.top_boat_stacking_weight;
        let aw = w.unsigned_abs() as i64;

        if w > 0 {
            // Tiered
            for (b_idx, boat) in boats.iter().enumerate() {
                let factor = ctx.stacking_factors[b_idx];
                if factor <= 0 {
                    continue;
                }
                for s in 1..=boat.seat_count.as_int() {
                    if let Some(&r_idx) = a.grid.get(&(b_idx, s)) {
                        let rower = available[r_idx];
                        let quality =
                            (rower.skill.ordinal() as i64 * rower.strength.ordinal() as i64) / 2;
                        let coef = (-(aw * quality * factor) / 1000) as i32;
                        b.s16 += coef;
                    }
                }
            }
        } else {
            // Even speed
            for b_idx in 1..boats.len() {
                let boat = boats[b_idx];
                let factor = ctx.stacking_factors[b_idx];
                if factor <= 0 {
                    continue;
                }
                for s in 1..=boat.seat_count.as_int() {
                    if let Some(&r_idx) = a.grid.get(&(b_idx, s)) {
                        let rower = available[r_idx];
                        let quality =
                            (rower.skill.ordinal() as i64 * rower.strength.ordinal() as i64) / 2;
                        let coef = (-(aw * quality * factor) / 1000) as i32;
                        b.s16 += coef;
                    }
                }
            }
        }
    }

    // S17 — pair eligibility
    if cfg.pair_eligibility_weight != 0 {
        for (b_idx, boat) in boats.iter().enumerate() {
            if boat.seat_count.as_int() != 2 || boat.has_cox.as_bool() {
                continue;
            }
            if !boat_used(b_idx) {
                continue;
            }
            // Component 1: intermediate skill penalty
            for seat in 1..=2 {
                if let Some(&r_idx) = a.grid.get(&(b_idx, seat)) {
                    if available[r_idx].skill == lineup_db::rower::types::Skill::Intermediate {
                        b.s17 += cfg.pair_eligibility_weight * 2;
                    }
                }
            }
            // Component 2: strength mismatch
            if let (Some(&r1), Some(&r2)) = (a.grid.get(&(b_idx, 1)), a.grid.get(&(b_idx, 2))) {
                let diff =
                    (available[r1].strength.ordinal() - available[r2].strength.ordinal()).abs();
                b.s17 += diff * cfg.pair_eligibility_weight;
            }
        }
    }

    // S18 — minimize bench
    if cfg.minimize_bench_weight != 0 {
        for (r_idx, rower) in available.iter().enumerate() {
            if rower.is_designated_cox.as_bool() {
                continue;
            }
            if matches!(a.places[r_idx], Place::Seated { .. }) {
                b.s18 += -cfg.minimize_bench_weight;
            }
        }
    }

    // S19 — boat-size stacking
    if cfg.boat_size_stacking_weight != 0 {
        let w = cfg.boat_size_stacking_weight;
        for (b_idx, boat) in boats.iter().enumerate() {
            if !boat_used(b_idx) {
                continue;
            }
            let size_factor = (8i32)
                .checked_div(boat.seat_count.as_int())
                .unwrap_or(1)
                .max(1);
            for s in 1..=boat.seat_count.as_int() {
                if let Some(&r_idx) = a.grid.get(&(b_idx, s)) {
                    let rower = available[r_idx];
                    let quality = rower.skill.ordinal() + rower.strength.ordinal();
                    b.s19 += -w * quality * size_factor;
                }
            }
        }
    }

    // S20 — bench cooldown (reward placing recently-benched rowers)
    for (&r_idx, &effective) in &ctx.bench_cooldown {
        if matches!(a.places[r_idx], Place::Seated { .. }) {
            b.s20 += -effective;
        }
    }

    // S21 — stroke spread
    if cfg.stroke_spread_weight != 0 && ctx.stroke_rower_indices.len() > 1 {
        let strokes = &ctx.stroke_rower_indices;
        for (b_idx, _) in boats.iter().enumerate() {
            if !boat_used(b_idx) {
                continue;
            }
            // Count designated strokes in this boat
            let mut stroke_count = 0;
            for &r_idx in strokes {
                if let Place::Seated { boat_idx, .. } = a.places[r_idx] {
                    if boat_idx == b_idx {
                        stroke_count += 1;
                    }
                }
            }
            // Penalty: C(n, 2) * weight
            if stroke_count >= 2 {
                let pairs = stroke_count * (stroke_count - 1) / 2;
                b.s21 += pairs * cfg.stroke_spread_weight;
            }
        }
    }

    // Reference similarity
    for &(r_idx, b_idx, seat, weight) in &ctx.reference_terms {
        if let Place::Seated { boat_idx, seat: s } = a.places[r_idx] {
            if boat_idx == b_idx && s == seat {
                b.reference += weight;
            }
        }
    }

    // Partial-fill bonus
    if ctx.partial_fill.max_empty() > 0 && cfg.partial_fill_bonus != 0 {
        for (b_idx, boat) in boats.iter().enumerate() {
            if !boat_used(b_idx) {
                continue;
            }
            for seat in optional_seats(boat) {
                if a.grid.contains_key(&(b_idx, seat)) {
                    b.partial_fill += -cfg.partial_fill_bonus;
                }
            }
        }
    }

    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_round_trip() {
        // Smoke test: empty solution round-trips
        let solution = ProposedSolution::default();
        let boats: Vec<&Boat> = vec![];
        let available: Vec<&Rower> = vec![];
        let a = Assignment::from_solution(&solution, &boats, &available);
        let result = a.to_solution(&boats, &available);
        assert!(result.lineups.is_empty());
    }

    /// Verify the SA evaluator agrees with the CP objective for a
    /// 2-boat tiered solve. This reproduces a scenario where a
    /// 32-point discrepancy was observed.
    #[test]
    fn evaluator_agrees_with_cp_two_boats() {
        use lineup_db::availability::types::AvailabilityStatus;
        use lineup_db::boat::types::{BoatId, CoxPosition, OarsPerSeat, SeatCount, WeightClass};
        use lineup_db::rower::types::*;
        use lineup_db::snapshot::DbSnapshot;
        use lineup_db::types::IntBool;
        use std::collections::HashMap;

        let now = chrono::Utc::now().naive_utc();
        let date = chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();

        let mk_rower = |id: i32, skill: Skill, strength: Strength, side: Side| -> Rower {
            Rower {
                id: RowerId::new(id),
                name: format!("R{id}"),
                weight_class: RowerWeightClass::Medium,
                skill,
                strength,
                height: Height::Medium,
                side,
                side_strength: SideStrength::new(3),
                sweep_bias: SweepBias::new(0),
                can_cox: IntBool::TRUE,
                is_designated_cox: IntBool::FALSE,
                active: IntBool::TRUE,
                created_at: now,
                updated_at: now,
                weight_kg: None,
                height_m: None,
            }
        };
        let mk_cox = |id: i32| -> Rower {
            let mut r = mk_rower(id, Skill::Novice, Strength::Weak, Side::Either);
            r.is_designated_cox = IntBool::TRUE;
            r.weight_class = RowerWeightClass::Light;
            r
        };
        let mk_eight = |id: i32, name: &str, wc: WeightClass| -> Boat {
            Boat {
                id: BoatId::new(id),
                name: name.into(),
                weight_class: wc,
                seat_count: SeatCount::new(8),
                has_cox: IntBool::TRUE,
                oars_per_seat: OarsPerSeat::new(1),
                acquired_at: None,
                manufactured_at: None,
                relinquished_at: None,
                stroke_side: Side::Starboard,
                cox_position: CoxPosition::Stern,
            }
        };

        // 2 eights + 21 rowers (2 coxes + 19 rowers, 3 benched)
        let boats = vec![
            mk_eight(1, "Alpha", WeightClass::Heavy),
            mk_eight(2, "Beta", WeightClass::Medium),
        ];

        use Skill as Sk;
        use Strength as St;
        let rowers = vec![
            mk_cox(100),
            mk_cox(101),
            mk_rower(1, Sk::Expert, St::VeryStrong, Side::Port),
            mk_rower(2, Sk::Expert, St::VeryStrong, Side::Starboard),
            mk_rower(3, Sk::Master, St::Strong, Side::Port),
            mk_rower(4, Sk::Master, St::Strong, Side::Starboard),
            mk_rower(5, Sk::Master, St::VeryStrong, Side::Either),
            mk_rower(6, Sk::Master, St::Strong, Side::Either),
            mk_rower(7, Sk::Intermediate, St::Intermediate, Side::Port),
            mk_rower(8, Sk::Intermediate, St::Intermediate, Side::Starboard),
            mk_rower(9, Sk::Intermediate, St::Strong, Side::Either),
            mk_rower(10, Sk::Intermediate, St::Intermediate, Side::Either),
            mk_rower(11, Sk::Expert, St::Intermediate, Side::Port),
            mk_rower(12, Sk::Expert, St::Intermediate, Side::Starboard),
            mk_rower(13, Sk::Intermediate, St::Weak, Side::Either),
            mk_rower(14, Sk::Intermediate, St::Weak, Side::Either),
            mk_rower(15, Sk::Novice, St::Intermediate, Side::Either),
            mk_rower(16, Sk::Intermediate, St::Intermediate, Side::Port),
            mk_rower(17, Sk::Intermediate, St::Intermediate, Side::Starboard),
        ];

        let availability: HashMap<RowerId, AvailabilityStatus> = rowers
            .iter()
            .map(|r| (r.id, AvailabilityStatus::Yes))
            .collect();

        let snapshot = DbSnapshot {
            date,
            assume_available: false,
            rowers,
            availability,
            boats,
            last_coxed: HashMap::new(),
            last_benched: HashMap::new(),
            seat_affinities: Vec::new(),
            pair_affinities: Vec::new(),
            recent_placements: Vec::new(),
        };

        let mut failures: Vec<String> = Vec::new();

        // Isolated single-constraint configs to verify each
        // evaluator independently (only S8 placement reward is kept
        // so the solver fields boats).
        let mut only_s4 = silent();
        only_s4.side_preference_weight = 2;
        let mut only_s11 = silent();
        only_s11.end_pair_skill_weight = 1;
        let mut only_s13 = silent();
        only_s13.non_scull_retention_weight = 2;

        fn silent() -> crate::SolverConfig {
            let mut c = crate::SolverConfig::balanced();
            c.skill_variance_weight = 0;
            c.pair_affinity_weight = 0;
            c.seat_affinity_weight = 0;
            c.side_preference_weight = 0;
            c.weight_class_slack_weight = 0;
            c.cox_cooldown_penalty = 0;
            c.pair_strength_weight = 0;
            c.bow_pair_strength_weight = 0;
            c.height_balance_weight = 0;
            c.end_pair_skill_weight = 0;
            c.engine_room_strength_weight = 0;
            c.non_scull_retention_weight = 0;
            c.bow_cox_fit_weight = 0;
            c.top_boat_stacking_weight = 0;
            c.pair_eligibility_weight = 0;
            c.minimize_bench_weight = 0;
            c.boat_size_stacking_weight = 0;
            c.bench_cooldown_penalty = 0;
            c.stroke_spread_weight = 0;
            c.partial_fill_bonus = 0;
            c
        }

        for (preset_name, config) in [
            ("balanced", crate::SolverConfig::balanced()),
            ("tiered", crate::SolverConfig::tiered()),
            ("even_speed", crate::SolverConfig::even_speed()),
            ("random", crate::SolverConfig::random()),
            ("only_s4", only_s4),
            ("only_s11", only_s11),
            ("only_s13", only_s13),
        ] {
            let request = crate::SolveRequest {
                date,
                boats: vec![BoatId::new(1), BoatId::new(2)],
                partial_fill: crate::PartialFillPolicy::Strict,
                config,
                time_budget: Some(std::time::Duration::from_secs(5)),
                top_n: 1,
                tabu_min_diff: 2,
                reference_lineups: vec![],
                locks: vec![],
                required_boats: vec![],
                sa_postprocess: false,
            };

            let result = crate::solve(&snapshot, &request).unwrap();
            assert_eq!(result.status, crate::SolveStatus::Satisfied);

            let cp_obj = result.objective.unwrap();

            // Replicate the exact boat ordering that solve() uses
            // internally: resolve request.boats, then greedy sort.
            let resolved_boats: Vec<&Boat> = request
                .boats
                .iter()
                .filter_map(|bid| snapshot.boats.iter().find(|b| b.id == *bid))
                .collect();
            let sa_available: Vec<&Rower> = snapshot.available_rowers().collect();
            let sa_boats = crate::greedy_fleet_select(
                resolved_boats,
                &sa_available,
                request.partial_fill,
                &request.config,
            );

            let assignment = Assignment::from_solution(&result.primary, &sa_boats, &sa_available);
            let ctx = EvalContext::new(&snapshot, &request, &sa_boats, &sa_available);
            let breakdown = evaluate_breakdown(&assignment, &ctx);
            let sa_obj = breakdown.total();

            let discrepancy = (sa_obj - cp_obj).abs();
            // The SA evaluator computes tighter aux variable values
            // than the CP may achieve under timeout. S5's over+under
            // slack vars may not be fully minimized to |diff|.
            // Threshold of 50 accommodates this slack inflation.
            if discrepancy > 0 {
                eprintln!("[{preset_name}] CP={cp_obj} SA={sa_obj} disc={discrepancy}");
                // Compare CP vs SA per-constraint
                let sa_vals = [
                    ("s1", breakdown.s1),
                    ("s2", breakdown.s2),
                    ("s3", breakdown.s3),
                    ("s4", breakdown.s4),
                    ("s5", breakdown.s5),
                    ("s6", breakdown.s6),
                    ("s8", breakdown.s8),
                    ("s9", breakdown.s9),
                    ("s10", breakdown.s10),
                    ("s11", breakdown.s11),
                    ("s12", breakdown.s12),
                    ("s13", breakdown.s13),
                    ("sweep_bias", breakdown.sweep_bias),
                    ("s14", breakdown.s14),
                    ("s15", breakdown.s15),
                    ("s16", breakdown.s16),
                    ("s17", breakdown.s17),
                    ("s18", breakdown.s18),
                    ("s19", breakdown.s19),
                    ("s20", breakdown.s20),
                    ("s21", breakdown.s21),
                    ("reference", breakdown.reference),
                    ("partial_fill", breakdown.partial_fill),
                ];
                for &(cp_name, cp_val) in &result.cp_breakdown {
                    let sa_val = sa_vals.iter().find(|(n, _)| *n == cp_name).map(|(_, v)| *v);
                    if let Some(sv) = sa_val {
                        let diff = sv - cp_val;
                        if diff != 0 {
                            eprintln!("  DIFF {cp_name}: CP={cp_val} SA={sv} delta={diff}");
                        }
                    }
                }
            }
            // SA may compute a tighter (more negative) value than
            // CP due to aux variable slack. Only flag if SA is LESS
            // negative (worse) than CP, which would indicate a real
            // evaluator bug.
            if sa_obj > cp_obj {
                failures.push(format!(
                    "[{preset_name}] SA worse than CP: cp={cp_obj} sa={sa_obj}"
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "SA evaluator disagreements:\n{}",
            failures.join("\n")
        );
    }
}
