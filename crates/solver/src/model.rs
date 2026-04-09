//! Lineup solver model builder + shared helpers.
//!
//! The [`ModelBuilder`] struct owns every piece of mutable state the
//! Pumpkin model needs while it's being constructed (the `Solver`
//! itself, the `x[r, b, s]` assignment matrix, the per-boat
//! `use[b]` vars, the objective-terms accumulator, the three
//! `seat_trait_by_seat` shared maps, and a few auxiliary bags like
//! `wrong_side_by_rower`). Its methods incrementally add variables
//! and constraints; the top-level `solve` entry in `lib.rs` just
//! orchestrates them in order and hands the result off to the
//! optimiser.
//!
//! This module also holds the pure, stateless bits of the solver
//! (seat-position enumeration, boat class → weight-target ordinal,
//! rower seat eligibility, wrong-side penalty, and the per-seat
//! trait-aggregation factory). They stay as free functions because
//! they don't need solver state and several are used by helpers on
//! `ModelBuilder` itself.

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use lineup_db::boat::{types::WeightClass, Boat};
use lineup_db::rower::{types::Side, Rower};
use pumpkin_core::variables::{AffineView, DomainId, TransformableVariable};
use pumpkin_core::Solver;

use crate::{PartialFillPolicy, SolverConfig};

/// Owns every piece of mutable state the Pumpkin model needs during
/// construction. Methods incrementally add variables and post
/// constraints; the top-level `solve` entry just runs them in the
/// required order and then hands `self.solver` and `self.objective`
/// to Pumpkin's optimisation pipeline.
///
/// The lifetime `'a` is the lifetime of the snapshot the caller
/// derived `boats` and `available` from — typically the body of
/// `solve` itself.
pub(crate) struct ModelBuilder<'a> {
    pub(crate) solver: Solver,
    pub(crate) boats: Vec<&'a Boat>,
    pub(crate) available: Vec<&'a Rower>,
    pub(crate) cfg: SolverConfig,
    /// `use[boat_idx] ∈ {0, 1}` — whether the solver chose to field
    /// this candidate boat today. Drives boat selection and gates
    /// the H5 weight-class wall + S5 slack equality.
    pub(crate) use_b: Vec<DomainId>,
    /// Assignment matrix keyed by `(rower_idx, boat_idx,
    /// seat_position)`. Only eligible combinations exist as entries.
    pub(crate) x: BTreeMap<(usize, usize, i32), DomainId>,
    /// Weighted objective terms. Every soft constraint pushes one or
    /// more pre-scaled `AffineView`s here; at the end the top-level
    /// `solve` links the objective variable to their sum.
    pub(crate) obj_terms: Vec<AffineView<DomainId>>,
    /// Per-rower list of `x[r, b, s]` vars where the placement
    /// would be on the wrong side — populated during variable
    /// creation, consumed by S4 aggregation.
    pub(crate) wrong_side_by_rower: BTreeMap<usize, Vec<DomainId>>,
    /// Shared `seat_skill[b, s]` vars used by S1 (per-boat spread)
    /// and S11 (end-pair skill reward). Built on demand by the
    /// seat-trait prelude.
    pub(crate) seat_skill_by_seat: BTreeMap<(usize, i32), DomainId>,
    /// Shared `seat_strength[b, s]` vars used by S9/S9b (pair
    /// strength) and S12 (engine-room strength reward).
    pub(crate) seat_strength_by_seat: BTreeMap<(usize, i32), DomainId>,
    /// Shared `seat_height[b, s]` vars used by S10 (pair height
    /// balance).
    pub(crate) seat_height_by_seat: BTreeMap<(usize, i32), DomainId>,
}

impl<'a> ModelBuilder<'a> {
    /// Set up a fresh Pumpkin `Solver`, allocate the per-boat `use[b]`
    /// decision variables, and return an empty builder ready to have
    /// its variable-creation and constraint-posting methods called.
    pub(crate) fn new(
        boats: Vec<&'a Boat>,
        available: Vec<&'a Rower>,
        cfg: SolverConfig,
    ) -> Self {
        let mut solver = Solver::default();
        // `use[b] ∈ {0, 1}` for each candidate boat. Ordering matches
        // the `boats` vec so `use_b[b_idx]` is always the right var.
        let use_b: Vec<DomainId> = boats
            .iter()
            .map(|_| solver.new_bounded_integer(0, 1))
            .collect();
        Self {
            solver,
            boats,
            available,
            cfg,
            use_b,
            x: BTreeMap::new(),
            obj_terms: Vec::new(),
            wrong_side_by_rower: BTreeMap::new(),
            seat_skill_by_seat: BTreeMap::new(),
            seat_strength_by_seat: BTreeMap::new(),
            seat_height_by_seat: BTreeMap::new(),
        }
    }

    /// Create one `x[r, b, s] ∈ {0, 1}` variable per eligible
    /// (rower, boat, seat) triple. Ineligible combinations simply
    /// don't exist in the model — cleaner than dead vars with a
    /// fixed-zero domain. While iterating we also bucket every
    /// wrong-side candidate into `wrong_side_by_rower` for S4
    /// aggregation to consume.
    ///
    /// This must run before any constraint block that reads `self.x`.
    pub(crate) fn create_variables(&mut self) {
        for (b_idx, boat) in self.boats.iter().enumerate() {
            for seat in seat_positions(boat) {
                for (r_idx, rower) in self.available.iter().enumerate() {
                    if !rower_eligible_for_seat(rower, boat, seat) {
                        continue;
                    }
                    let var = self.solver.new_bounded_integer(0, 1);
                    self.x.insert((r_idx, b_idx, seat), var);

                    // S4: collect wrong-side placements for per-rower
                    // aggregation rather than pushing a term per variable.
                    if wrong_side_penalty(rower, boat, seat) > 0 {
                        self.wrong_side_by_rower
                            .entry(r_idx)
                            .or_default()
                            .push(var);
                    }
                }
            }
        }
    }

    /// H1 — seat fill conditional on `use[b]`, plus the optional
    /// partial-fill cap.
    ///
    /// For each REQUIRED `(boat, seat)`:
    ///   `Σ_r x[r, b, s] = use[b]`
    ///
    /// For each OPTIONAL `(boat, seat)` (under a non-strict partial-
    /// fill policy):
    ///   `Σ_r x[r, b, s] ≤ use[b]`
    ///
    /// When a boat has a seat with no eligible rower at all, we
    /// force `use[b] = 0` (the boat can never be fielded) and skip
    /// the rest of its seats.
    ///
    /// Finally, when `k > 0` and the boat has optional seats, post
    /// the "at most k empty optional seats" cap:
    ///   `Σ_{s ∈ opt_seats, r} x[r, b, s] ≥ (n_opt − k) · use[b]`
    pub(crate) fn post_h1_seat_fill(
        &mut self,
        partial_fill: PartialFillPolicy,
    ) -> Result<()> {
        let k_allowed = partial_fill.max_empty();
        for (b_idx, boat) in self.boats.iter().enumerate() {
            let opt_seats = optional_seats(boat);
            let mut force_unused = false;
            for seat in seat_positions(boat) {
                let mut terms: Vec<AffineView<DomainId>> = (0..self.available.len())
                    .filter_map(|r_idx| {
                        self.x.get(&(r_idx, b_idx, seat)).map(|v| v.scaled(1))
                    })
                    .collect();
                if terms.is_empty() {
                    // If no rower is eligible for this seat at all, the
                    // boat can never be used. Force use[b] = 0.
                    let tag = self.solver.new_constraint_tag();
                    self.solver
                        .add_constraint(pumpkin_constraints::equals(
                            vec![self.use_b[b_idx].scaled(1)],
                            0,
                            tag,
                        ))
                        .post()
                        .map_err(|e| {
                            anyhow!("posting boat-unusable constraint: {e:?}")
                        })?;
                    tracing::debug!(
                        boat = %boat.name,
                        seat,
                        "no eligible rower for seat; forcing boat unused"
                    );
                    force_unused = true;
                    break;
                }
                // Required: Σ x - use[b] = 0. Optional: Σ x - use[b] ≤ 0.
                terms.push(self.use_b[b_idx].scaled(-1));
                let tag = self.solver.new_constraint_tag();
                if opt_seats.contains(&seat) && k_allowed > 0 {
                    self.solver
                        .add_constraint(pumpkin_constraints::less_than_or_equals(
                            terms, 0, tag,
                        ))
                        .post()
                        .map_err(|e| {
                            anyhow!("posting optional seat-fill constraint: {e:?}")
                        })?;
                } else {
                    self.solver
                        .add_constraint(pumpkin_constraints::equals(terms, 0, tag))
                        .post()
                        .map_err(|e| {
                            anyhow!("posting seat-fill constraint: {e:?}")
                        })?;
                }
            }
            if force_unused {
                continue;
            }

            // Partial-fill cap: at least `(n_opt - k_allowed)` of the
            // optional seats must be filled when the boat is used.
            //
            //   Σ_{s ∈ opt_seats, r} x[r,b,s]  ≥  (n_opt - k) * use[b]
            //   ⇔  (n_opt - k) * use[b] - Σ x ≤ 0
            let n_opt = opt_seats.len() as i32;
            if k_allowed > 0 && n_opt > 0 {
                let k = k_allowed.min(n_opt);
                let min_filled_opt = n_opt - k;
                if min_filled_opt > 0 {
                    let mut cap_terms: Vec<AffineView<DomainId>> = Vec::new();
                    for s in &opt_seats {
                        for r_idx in 0..self.available.len() {
                            if let Some(&var) = self.x.get(&(r_idx, b_idx, *s)) {
                                cap_terms.push(var.scaled(-1));
                            }
                        }
                    }
                    cap_terms.push(self.use_b[b_idx].scaled(min_filled_opt));
                    let tag = self.solver.new_constraint_tag();
                    self.solver
                        .add_constraint(pumpkin_constraints::less_than_or_equals(
                            cap_terms, 0, tag,
                        ))
                        .post()
                        .map_err(|e| anyhow!("posting partial-fill cap: {e:?}"))?;
                }
            }
        }
        Ok(())
    }

    /// H2 — each rower occupies at most one seat in the entire
    /// fielded fleet:
    ///
    ///   `for each rower r:    Σ_{b, s} x[r, b, s] ≤ 1`
    ///
    /// Surplus rowers (available but no seat fits them) fall out
    /// naturally with `Σ = 0`.
    pub(crate) fn post_h2_at_most_one(&mut self) -> Result<()> {
        for r_idx in 0..self.available.len() {
            let terms: Vec<DomainId> = self
                .x
                .iter()
                .filter_map(|(&(r, _, _), &v)| if r == r_idx { Some(v) } else { None })
                .collect();
            if terms.is_empty() {
                continue;
            }
            let tag = self.solver.new_constraint_tag();
            self.solver
                .add_constraint(pumpkin_constraints::less_than_or_equals(terms, 1, tag))
                .post()
                .map_err(|e| anyhow!("posting rower-at-most-one constraint: {e:?}"))?;
        }
        Ok(())
    }

    /// H6 — global fleet capacity prune:
    ///
    ///   `Σ_b (seats_total[b] · use[b]) ≤ num_available`
    ///
    /// Without this, the solver explores many infeasible fleet
    /// configurations (e.g. "field 6 eights = 54 seats" with only
    /// 20 rowers available) before individual H1/H2 constraints
    /// prune them. The explicit global bound is a cheap prune that
    /// collapses the search space by an order of magnitude on
    /// realistic club fleets — see the bench notes in the README.
    pub(crate) fn post_h6_fleet_capacity(&mut self) -> Result<()> {
        let capacity_terms: Vec<_> = self
            .boats
            .iter()
            .enumerate()
            .map(|(b_idx, boat)| {
                let seats_total =
                    boat.seat_count + if boat.has_cox.as_bool() { 1 } else { 0 };
                self.use_b[b_idx].scaled(seats_total)
            })
            .collect();
        let tag = self.solver.new_constraint_tag();
        self.solver
            .add_constraint(pumpkin_constraints::less_than_or_equals(
                capacity_terms,
                self.available.len() as i32,
                tag,
            ))
            .post()
            .map_err(|e| anyhow!("posting fleet-capacity bound: {e:?}"))?;
        Ok(())
    }

    /// H5 + S5 — weight-class hard wall (upper bound) and the soft
    /// slack target.
    ///
    /// The hard wall is unconditional: `sum(ordinal · x) ≤
    /// target_sum + N`. Trivially satisfied when the boat is
    /// unused (`sum = 0`). No big-M — the lower "not too light"
    /// bound is delegated to S5 slack because conditional big-M
    /// constraints tanked Pumpkin's propagation in earlier
    /// experiments.
    ///
    /// S5 slack (conditional on `use[b]` via the equality form):
    ///   `sum(ordinal · x) − over[b] + under[b] = target_sum · use[b]`
    ///
    /// At optimum, `over = under = 0` when `use[b] = 0`. The
    /// slack terms are only posted when the soft weight is non-
    /// zero; the hard wall still applies regardless.
    pub(crate) fn post_h5_s5_weight_class(&mut self) -> Result<()> {
        for (b_idx, boat) in self.boats.iter().enumerate() {
            let n_rowing = boat.seat_count;
            if n_rowing == 0 {
                continue;
            }
            let target = boat_target_weight_ordinal(boat.weight_class);
            let target_sum = target * n_rowing;
            let wall = n_rowing;

            let positive_terms: Vec<_> = self
                .x
                .iter()
                .filter_map(|(&(r_idx, b, seat), &var)| {
                    if b != b_idx || seat == 0 {
                        return None;
                    }
                    Some(var.scaled(self.available[r_idx].weight_class.ordinal()))
                })
                .collect();
            if positive_terms.is_empty() {
                continue;
            }

            // Hard wall UPPER (unconditional).
            let tag_hi = self.solver.new_constraint_tag();
            self.solver
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
            if self.cfg.weight_class_slack_weight != 0 {
                let slack_upper = 3 * n_rowing;
                let over = self.solver.new_bounded_integer(0, slack_upper);
                let under = self.solver.new_bounded_integer(0, slack_upper);

                let mut eq_terms: Vec<_> = positive_terms.clone();
                eq_terms.push(over.scaled(-1));
                eq_terms.push(under.scaled(1));
                eq_terms.push(self.use_b[b_idx].scaled(-target_sum));
                let tag_eq = self.solver.new_constraint_tag();
                self.solver
                    .add_constraint(pumpkin_constraints::equals(eq_terms, 0, tag_eq))
                    .post()
                    .map_err(|e| anyhow!("weight-class slack equality: {e:?}"))?;

                self.obj_terms
                    .push(over.scaled(self.cfg.weight_class_slack_weight));
                self.obj_terms
                    .push(under.scaled(self.cfg.weight_class_slack_weight));
            }
        }
        Ok(())
    }
}

/// All seat positions on a boat, including the cox seat (0) for
/// coxed boats and the rowing seats 1..=seat_count.
pub(crate) fn seat_positions(boat: &Boat) -> Vec<i32> {
    let mut seats = Vec::with_capacity((boat.seat_count + 1) as usize);
    if boat.has_cox.as_bool() {
        seats.push(0);
    }
    for s in 1..=boat.seat_count {
        seats.push(s);
    }
    seats
}

/// Which rowing seats of a given boat are "optional" — i.e. may be
/// left empty under a non-strict [`crate::PartialFillPolicy`]. The
/// set is hardcoded per boat class based on common rowing practice:
///
/// - **8+**: seats 3 and 4 are the inside bow pair; these are the
///   conventional "row it down a pair" positions when the club is
///   short on rowers.
/// - **Everything else**: no optional seats. A 4-boat with a
///   missing seat is too unbalanced to be useful, and smaller boats
///   have no realistic partial-fill pattern.
pub(crate) fn optional_seats(boat: &Boat) -> Vec<i32> {
    match boat.seat_count {
        8 => vec![3, 4],
        _ => vec![],
    }
}

/// Solver target ordinal for a boat weight class. Matches
/// [`lineup_db::rower::types::RowerWeightClass::ordinal`] so the two
/// can be compared directly in the weight-class band constraint.
/// `Tubby` clamps to `Heavy` — we don't have a Tubby rower bucket,
/// so the boat just acts as "as heavy as Heavy".
pub(crate) fn boat_target_weight_ordinal(wc: WeightClass) -> i32 {
    match wc {
        WeightClass::Light => 1,
        WeightClass::Medium => 2,
        WeightClass::Heavy => 3,
        WeightClass::Tubby => 3,
    }
}

/// Whether a rower is eligible to occupy a given seat on a given
/// boat. Filters out structurally-impossible combinations at
/// variable-creation time so the Pumpkin model never has to carry
/// dead `x` vars with a fixed-at-zero domain.
pub(crate) fn rower_eligible_for_seat(rower: &Rower, boat: &Boat, seat: i32) -> bool {
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

/// How many penalty points a (rower, boat, seat) placement
/// contributes to the S4 soft-side objective. Returns 0 for the cox
/// seat, for `Either` rowers, and for correct-side placements;
/// otherwise returns the rower's `side_strength` (which is guaranteed
/// ≥ 1 here because the eligibility filter already rejected
/// `SideStrength::HARD` mismatches).
pub(crate) fn wrong_side_penalty(rower: &Rower, boat: &Boat, seat: i32) -> i32 {
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
pub(crate) fn build_seat_trait_map(
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
