//! Fleet-level soft constraints: S4 wrong-side aggregation, S6 cox
//! cooldown, S8 placement reward, S13 retention, sweep-bias
//! alignment penalty, and reference-lineup similarity (unified
//! novelty / baseline).
//!
//! "Fleet-level" here means "operates at the boat / rower level
//! rather than per-seat or per-partition". The seat-level softs
//! (S1 skill variance, S2 pair affinity, S3 seat affinity, S9 pair
//! strength, S10 pair height, S11 end-pair skill, S12 engine-room
//! strength) live in `soft_seats.rs`.
//!
//! All four methods extend `ModelBuilder` via a separate `impl`
//! block — Rust is happy to split `impl` blocks across modules as
//! long as they're in the same crate, so `lib.rs`'s `solve()` can
//! call them directly on the builder without touching anything in
//! this file.

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use lineup_db::snapshot::DbSnapshot;
use pumpkin_core::variables::{AffineView, DomainId, TransformableVariable};

use crate::model::ModelBuilder;
use crate::COX_COOLDOWN_DAYS;

impl<'a> ModelBuilder<'a> {
    /// S8 — reward fielding each boat by its total seat count
    /// (rowers + cox).
    ///
    /// Because H1 is all-or-nothing per boat (`use[b] = 1` implies
    /// every required seat is filled), rewarding "rowers placed" and
    /// rewarding `seats_total · use[b]` produce the same assignments
    /// at the optimum. The per-boat form pushes only `|boats|` terms
    /// into the final objective-link equality instead of one per x
    /// variable — a huge win for Pumpkin's linear propagator, which
    /// scales badly with term count in the objective sum.
    pub(crate) fn post_s8_placement_reward(&mut self) {
        if self.cfg.placement_reward_weight == 0 {
            return;
        }
        let mut count = 0usize;
        for (b_idx, boat) in self.boats.iter().enumerate() {
            let seats_total = boat.seat_count.as_int() + if boat.has_cox.as_bool() { 1 } else { 0 };
            // Scale the base reward by the boat-class bias: (1 + bias).
            // A bias of 0 = normal reward; positive = prefer this class.
            let class = crate::BoatClass::from_boat(boat);
            let bias = self.cfg.class_bias(class);
            let effective_weight = self.cfg.placement_reward_weight * (1 + bias);
            let coef = -seats_total * effective_weight;
            if coef != 0 {
                let var = self.use_b[b_idx];
                push_obj!(self.obj_terms, self.obj_term_evals, var, coef);
                count += 1;
            }
        }
        tracing::debug!(terms = count, "S8 placement reward");
    }

    /// S4 — aggregate wrong-side placements per rower into a single
    /// `wrong_count[r] ∈ {0, 1}` aux var and push one scaled
    /// objective term per rower, rather than a term per candidate x.
    ///
    /// Because H2 guarantees each rower occupies at most one seat in
    /// total, the sum `Σ wrong_side_x[r]` is at most 1, so the
    /// `[0, 1]` domain on `wrong_count` is tight and the link
    /// equality `Σ wrong_side_x[r] − wrong_count[r] = 0` is always
    /// satisfiable. This cuts the S4 contribution to `obj_terms`
    /// from `O(rowers × seats)` down to `O(rowers)`, which mattered
    /// enough at scale to motivate the refactor. See the README §S4
    /// performance note.
    pub(crate) fn post_s4_wrong_side(&mut self) -> Result<()> {
        if self.cfg.side_preference_weight == 0 {
            return Ok(());
        }
        let mut count = 0usize;
        let ModelBuilder {
            solver,
            available,
            obj_terms,
            obj_term_evals,
            wrong_side_by_rower,
            cfg,
            ..
        } = self;
        for (&r_idx, wrong_vars) in &*wrong_side_by_rower {
            if wrong_vars.is_empty() {
                continue;
            }
            let rower = available[r_idx];
            let coef = rower.side_strength.as_int() * cfg.side_preference_weight;
            if coef == 0 {
                // stored strength = 0 already meant "hard lock" so this shouldn't fire
                continue;
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

            push_obj!(obj_terms, obj_term_evals, wrong_count, coef);
            count += 1;
        }
        tracing::debug!(terms = count, "S4 wrong-side aggregation");
        Ok(())
    }

    /// S6 — cox cooldown. Non-designated rowers who coxed inside
    /// the rolling `COX_COOLDOWN_DAYS` window incur a penalty if
    /// the solver tries to seat them as cox again. Designated
    /// coxes are exempt.
    ///
    /// **Linear decay by days-since.** The effective penalty is
    ///
    ///   `effective = ceil(cox_cooldown_penalty × (COX_COOLDOWN_DAYS − days_since) / COX_COOLDOWN_DAYS)`
    ///
    /// so a rower who coxed yesterday pays close to the full
    /// configured penalty, a rower 13 days out pays 1, and a
    /// rower ≥ 14 days out drops out of the window and pays 0.
    /// This removes the cliff at the cooldown boundary and gives
    /// the solver a smooth preference for "least recently coxed"
    /// among multiple cooldown candidates (see [`S6 design
    /// note`](#s6-design-note) in the README). `ceil` rounds each
    /// rower up to at least 1 inside the window so no cooldown
    /// rower is silently free.
    ///
    /// **Encoding** mirrors S4's per-rower aggregation: gather
    /// the rower's cox-seat x vars across all coxed candidate
    /// boats (coxless boats don't create a seat-0 x), sum them
    /// into a `cox_use[r] ∈ {0, 1}` aux var via a link equality
    /// (by H2 the sum is ≤ 1), and push exactly one obj term per
    /// penalised rower scaled by their personal `effective`
    /// coefficient. Cost: `O(rowers in cooldown)`.
    pub(crate) fn post_s6_cox_cooldown(
        &mut self,
        snapshot: &DbSnapshot,
        date: NaiveDate,
    ) -> Result<()> {
        if self.cfg.cox_cooldown_penalty == 0 {
            return Ok(());
        }
        let mut count = 0usize;
        let ModelBuilder {
            solver,
            boats,
            available,
            x,
            obj_terms,
            obj_term_evals,
            cfg,
            ..
        } = self;
        for (r_idx, rower) in available.iter().enumerate() {
            if rower.is_designated_cox.as_bool() {
                continue; // exempt — designated coxes cox as often as needed
            }
            let Some(last_date) = snapshot.last_coxed.get(&rower.id) else {
                continue; // never coxed → no cooldown to enforce
            };
            let days_since = (date - *last_date).num_days();
            if days_since < 0 || days_since >= COX_COOLDOWN_DAYS {
                continue; // outside cooldown window (or in the future; ignore)
            }

            // Linear decay. Uses ceiling division so any rower
            // inside the window incurs at least 1 unit of penalty —
            // otherwise days 12-13 (for the default penalty of 5)
            // would produce 0 under floor division and Pumpkin's
            // `.scaled(0)` would panic anyway. Computed in i64 to
            // avoid overflow on exotic penalty values, then cast
            // back to i32 for the Pumpkin API.
            let numerator = cfg.cox_cooldown_penalty as i64 * (COX_COOLDOWN_DAYS - days_since);
            let effective = ((numerator + COX_COOLDOWN_DAYS - 1) / COX_COOLDOWN_DAYS) as i32;
            if effective <= 0 {
                // Defensive: only reachable if `cox_cooldown_penalty`
                // itself is zero or negative and the ceiling rounds
                // to <= 0. `.scaled(0)` panics in Pumpkin and a
                // negative penalty would invert the constraint, so
                // skip the push entirely.
                continue;
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

            push_obj!(obj_terms, obj_term_evals, cox_use, effective);
            count += 1;
        }
        tracing::debug!(terms = count, "S6 cox cooldown");
        Ok(())
    }

    /// Unified reference-lineup similarity scoring. Each
    /// [`ReferenceLineup`] carries a signed `weight`:
    ///
    /// - **Positive → avoid** (novelty). Each matching placement
    ///   adds `weight` to the objective (a penalty).
    /// - **Negative → prefer** (baseline / carry-forward). Each
    ///   matching placement adds `weight` (negative, so a reward).
    ///
    /// For each reference lineup, we iterate its placements, look
    /// up the corresponding `x[r, b, s]` variable (silently
    /// skipping absent rowers / missing boats / ineligible seats),
    /// and push `var.scaled(weight)` into `obj_terms`.
    ///
    /// This replaces both the old S7 (novelty) and S14 (baseline
    /// similarity) with a single linear mechanism. The caller
    /// decides what placements to include and what sign/magnitude
    /// the weight should have.
    pub(crate) fn post_reference_similarity(
        &mut self,
        references: &[crate::ReferenceLineup],
    ) -> Result<()> {
        let ModelBuilder {
            boats,
            available,
            x,
            obj_terms,
            obj_term_evals,
            ..
        } = self;
        for reference in references {
            if reference.weight == 0 {
                continue;
            }
            for p in &reference.placements {
                let Some(r_idx) = available.iter().position(|r| r.id == p.rower_id) else {
                    continue;
                };
                let Some(b_idx) = boats.iter().position(|b| b.id == p.boat_id) else {
                    continue;
                };
                if let Some(&var) = x.get(&(r_idx, b_idx, p.seat)) {
                    push_obj!(obj_terms, obj_term_evals, var, reference.weight);
                }
            }
        }
        Ok(())
    }

    /// S13 — retention. Rewards placing rowers, scaled by how
    /// strongly they prefer a particular boat type. Hard-locked
    /// rowers (`sweep_bias` ±2) get the strongest retention;
    /// ambivalent rowers (`sweep_bias` 0) get the weakest. This
    /// way the solver preferentially benches rowers who are
    /// flexible over those who have nowhere else to go.
    ///
    /// Scale: `abs(sweep_bias) + 1`, so 0→1, ±1→2, ±2→3.
    ///
    /// Encoding mirrors S4 / S6's per-rower aggregation — one
    /// `rower_used[r] ∈ {0, 1}` aux var per available rower,
    /// linked to `Σ_{b, s} x[r, b, s]` by an equality. By H2
    /// the sum is at most 1, so the `[0, 1]` domain is tight.
    /// Push exactly one obj term per rower:
    ///
    ///   `obj_terms.push(rower_used[r].scaled(-retention_weight * scale))`
    ///
    /// Total contribution is O(available rowers) obj terms.
    pub(crate) fn post_s13_non_scull_retention(&mut self) -> Result<()> {
        if self.cfg.non_scull_retention_weight == 0 {
            return Ok(());
        }
        let mut count = 0usize;
        let ModelBuilder {
            solver,
            available,
            x,
            obj_terms,
            obj_term_evals,
            cfg,
            ..
        } = self;
        for (r_idx, rower) in available.iter().enumerate() {
            let scale = rower.sweep_bias.as_int().unsigned_abs() as i32 + 1;
            // Collect every x variable for this rower. H2 bounds
            // the sum at 1, so the aggregated aux var's [0, 1]
            // domain is tight without any explicit upper.
            let rower_vars: Vec<DomainId> = x
                .iter()
                .filter_map(|(&(r, _, _), &v)| if r == r_idx { Some(v) } else { None })
                .collect();
            if rower_vars.is_empty() {
                // Rower has no eligible (r, b, s) triples — they
                // can't be placed at all, so there's no "use vs
                // bench" decision to bias and no aux var to
                // create.
                continue;
            }

            let rower_used = solver.new_bounded_integer(0, 1);
            let mut link: Vec<AffineView<DomainId>> =
                rower_vars.iter().map(|v| v.scaled(1)).collect();
            link.push(rower_used.scaled(-1));
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(link, 0, tag))
                .post()
                .map_err(|e| anyhow!("S13 rower-use link: {e:?}"))?;

            push_obj!(
                obj_terms,
                obj_term_evals,
                rower_used,
                -cfg.non_scull_retention_weight * scale
            );
            count += 1;
        }
        tracing::debug!(terms = count, "S13 non-scull retention");
        Ok(())
    }

    /// Sweep-bias alignment penalty. Penalises placing a rower in
    /// a boat type that conflicts with their `sweep_bias`:
    ///
    /// - Rower with `sweep_bias > 0` in a scull boat: penalty =
    ///   `sweep_bias * non_scull_retention_weight`
    /// - Rower with `sweep_bias < 0` in a sweep boat: penalty =
    ///   `-sweep_bias * non_scull_retention_weight`
    ///
    /// Hard-locked rowers (±2) are already filtered out of
    /// wrong-type boats by the eligibility gate in
    /// `create_variables`, so only ±1 values generate penalty
    /// terms here. Pushes one scaled term per mismatched x
    /// variable — no aux vars needed.
    pub(crate) fn post_sweep_bias_penalty(&mut self) {
        if self.cfg.non_scull_retention_weight == 0 {
            return;
        }
        let mut count = 0usize;
        for (b_idx, boat) in self.boats.iter().enumerate() {
            let boat_is_scull = boat.is_scull();
            let boat_is_sweep = boat.is_sweep();
            if !boat_is_scull && !boat_is_sweep {
                continue; // shouldn't happen, but guard
            }
            for (r_idx, rower) in self.available.iter().enumerate() {
                let bias = rower.sweep_bias.as_int();
                let penalty = if boat_is_scull && bias > 0 {
                    bias * self.cfg.non_scull_retention_weight
                } else if boat_is_sweep && bias < 0 {
                    -bias * self.cfg.non_scull_retention_weight
                } else {
                    0
                };
                if penalty == 0 {
                    continue;
                }
                // Find all x vars for this (rower, boat) and push
                // penalty terms. Most rowers will have 0 or a few.
                for s in 0..=(boat.seat_count.as_int()) {
                    if let Some(&var) = self.x.get(&(r_idx, b_idx, s)) {
                        push_obj!(self.obj_terms, self.obj_term_evals, var, penalty);
                        count += 1;
                    }
                }
            }
        }
        tracing::debug!(terms = count, "sweep-bias penalty");
    }

    /// S14 — bow-loader cox fit penalty. Penalises tall and heavy
    /// rowers in the cox seat (position 0) of bow-loader boats.
    /// Height is the primary factor; weight is secondary.
    ///
    /// Penalty table (multiplied by `bow_cox_fit_weight`):
    /// - Tall: 3, Very tall: 5
    /// - Heavy: 1
    ///
    /// Only applies when `boat.cox_position == CoxPosition::Bow`.
    /// Stern-loader cox seats have no size constraint.
    pub(crate) fn post_s14_bow_cox_fit(&mut self) -> Result<()> {
        use lineup_db::boat::types::CoxPosition;
        use lineup_db::rower::types::Height;

        if self.cfg.bow_cox_fit_weight == 0 {
            return Ok(());
        }
        let mut count = 0usize;
        let w = self.cfg.bow_cox_fit_weight;

        for (b_idx, boat) in self.boats.iter().enumerate() {
            if !boat.has_cox.as_bool() || boat.cox_position != CoxPosition::Bow {
                continue;
            }
            for (r_idx, rower) in self.available.iter().enumerate() {
                let Some(&var) = self.x.get(&(r_idx, b_idx, 0)) else {
                    continue;
                };

                let height_penalty = match rower.height {
                    Height::Tall => 3,
                    Height::VeryTall => 5,
                    _ => 0,
                };
                let weight_penalty = match rower.weight_class.ordinal() {
                    3 => 1, // Heavy
                    _ => 0,
                };
                let total = height_penalty + weight_penalty;
                if total == 0 {
                    continue;
                }
                let scaled = total * w;
                if scaled == 0 {
                    continue;
                }
                push_obj!(self.obj_terms, self.obj_term_evals, var, scaled);
                count += 1;
            }
        }
        tracing::debug!(terms = count, "S14 bow-cox fit");
        Ok(())
    }

    /// S15 — designated-cox retention. Strongly rewards placing
    /// designated coxswains in cox seats. A designated cox who gets
    /// benched has nowhere else to go (they can't row), so this is
    /// effectively a hard preference. The weight is intentionally
    /// high so it takes priority over most other soft constraints.
    pub(crate) fn post_s15_designated_cox_retention(&mut self) -> Result<()> {
        let mut count = 0usize;
        let ModelBuilder {
            solver,
            available,
            x,
            obj_terms,
            obj_term_evals,
            ..
        } = self;

        for (r_idx, rower) in available.iter().enumerate() {
            if !rower.is_designated_cox.as_bool() {
                continue;
            }
            // Collect all cox-seat x variables for this rower.
            let cox_vars: Vec<DomainId> = x
                .iter()
                .filter_map(
                    |(&(r, _, s), &v)| {
                        if r == r_idx && s == 0 {
                            Some(v)
                        } else {
                            None
                        }
                    },
                )
                .collect();
            if cox_vars.is_empty() {
                continue;
            }

            let cox_used = solver.new_bounded_integer(0, 1);
            let mut link: Vec<AffineView<DomainId>> =
                cox_vars.iter().map(|v| v.scaled(1)).collect();
            link.push(cox_used.scaled(-1));
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(link, 0, tag))
                .post()
                .map_err(|e| anyhow!("S15 designated-cox-use link: {e:?}"))?;

            // Strong negative weight = reward for placing.
            // 10 is higher than most other soft constraints to ensure
            // designated coxes are placed before anything else.
            push_obj!(obj_terms, obj_term_evals, cox_used, -10);
            count += 1;
        }
        tracing::debug!(terms = count, "S15 designated-cox retention");
        Ok(())
    }

    /// S17 — pair-eligibility soft constraints for pair boats
    /// (seat_count=2, no cox). Two components:
    ///
    /// 1. **Skill penalty:** Intermediate rowers incur a penalty when
    ///    placed in a pair. Master/Expert = 0 penalty. (Novices are
    ///    hard-gated by H7 in `rower_eligible_for_seat`.)
    ///
    /// 2. **Strength-mismatch penalty:** the absolute difference in
    ///    strength ordinals between the two rowers in a pair, scaled
    ///    by the weight. Balanced pairs row better.
    pub(crate) fn post_s17_pair_eligibility(&mut self) -> Result<()> {
        if self.cfg.pair_eligibility_weight == 0 {
            return Ok(());
        }
        let mut count = 0usize;
        let ModelBuilder {
            solver,
            boats,
            available,
            x,
            obj_terms,
            obj_term_evals,
            cfg,
            seat_strength_by_seat,
            ..
        } = self;
        let w = cfg.pair_eligibility_weight;

        for (b_idx, boat) in boats.iter().enumerate() {
            // Only pair boats (2 rowing seats, no cox).
            if boat.seat_count.as_int() != 2 || boat.has_cox.as_bool() {
                continue;
            }

            // Component 1: per-rower skill penalty for Intermediate.
            for (r_idx, rower) in available.iter().enumerate() {
                for seat in 1..=2 {
                    if let Some(&var) = x.get(&(r_idx, b_idx, seat)) {
                        let penalty = match rower.skill {
                            lineup_db::rower::types::Skill::Intermediate => 2,
                            _ => 0, // Novice hard-gated, Master/Expert = free
                        };
                        if penalty > 0 {
                            push_obj!(obj_terms, obj_term_evals, var, w * penalty);
                            count += 1;
                        }
                    }
                }
            }

            // Component 2: strength mismatch between the two seats.
            // Uses the shared seat_strength trait map (built by
            // build_seat_trait_maps). strength[b,1] and strength[b,2]
            // are [1..4] aux vars linked to the placed rower's ordinal.
            let s1 = seat_strength_by_seat.get(&(b_idx, 1)).copied();
            let s2 = seat_strength_by_seat.get(&(b_idx, 2)).copied();
            if let (Some(str1), Some(str2)) = (s1, s2) {
                // diff >= str1 - str2 AND diff >= str2 - str1
                // => diff = |str1 - str2|.
                let diff = solver.new_bounded_integer(0, 3);
                let tag = solver.new_constraint_tag();
                // diff >= str1 - str2  →  diff - str1 + str2 >= 0
                solver
                    .add_constraint(pumpkin_constraints::less_than_or_equals(
                        vec![str1.scaled(1), str2.scaled(-1), diff.scaled(-1)],
                        0,
                        tag,
                    ))
                    .post()
                    .map_err(|e| anyhow!("S17 pair strength diff (a): {e:?}"))?;
                // diff >= str2 - str1  →  diff - str2 + str1 >= 0
                let tag2 = solver.new_constraint_tag();
                solver
                    .add_constraint(pumpkin_constraints::less_than_or_equals(
                        vec![str2.scaled(1), str1.scaled(-1), diff.scaled(-1)],
                        0,
                        tag2,
                    ))
                    .post()
                    .map_err(|e| anyhow!("S17 pair strength diff (b): {e:?}"))?;

                push_obj!(obj_terms, obj_term_evals, diff, w);
                count += 1;
            }
        }
        tracing::debug!(terms = count, "S17 pair eligibility");
        Ok(())
    }

    /// S18 — minimize bench. Per-rower reward for being placed in
    /// any seat. Applies to ALL available rowers (unlike S13 which
    /// only covers non-scull rowers). Higher weight = stronger
    /// pressure to field everyone.
    pub(crate) fn post_s18_minimize_bench(&mut self) -> Result<()> {
        if self.cfg.minimize_bench_weight == 0 {
            return Ok(());
        }
        let mut count = 0usize;
        let ModelBuilder {
            solver,
            available,
            x,
            obj_terms,
            obj_term_evals,
            cfg,
            ..
        } = self;
        for (r_idx, rower) in available.iter().enumerate() {
            // Skip designated coxes — they're handled by S15 and
            // can only go in cox seats, not rowing seats.
            if rower.is_designated_cox.as_bool() {
                continue;
            }
            let rower_vars: Vec<DomainId> = x
                .iter()
                .filter_map(|(&(r, _, _), &v)| if r == r_idx { Some(v) } else { None })
                .collect();
            if rower_vars.is_empty() {
                continue;
            }

            let rower_used = solver.new_bounded_integer(0, 1);
            let mut link: Vec<AffineView<DomainId>> =
                rower_vars.iter().map(|v| v.scaled(1)).collect();
            link.push(rower_used.scaled(-1));
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(link, 0, tag))
                .post()
                .map_err(|e| anyhow!("S18 rower-use link: {e:?}"))?;

            push_obj!(
                obj_terms,
                obj_term_evals,
                rower_used,
                -cfg.minimize_bench_weight
            );
            count += 1;
        }
        tracing::debug!(terms = count, "S18 minimize bench");
        Ok(())
    }

    /// S20 — bench cooldown. Penalises benching a rower who was
    /// benched at a recent committed practice. Uses the same
    /// linear-decay pattern as S6 (cox cooldown): rowers benched
    /// recently pay more, rowers benched longer ago pay less.
    ///
    /// The penalty is applied by *not* rewarding placement — i.e.
    /// if a recently-benched rower is benched again, the solver pays
    /// a penalty. Encoding: for each rower in the cooldown window,
    /// create a `rower_used ∈ {0,1}` aux var, then push
    /// `rower_used.scaled(-effective)` so the solver is rewarded
    /// for placing them (= penalised for benching them again).
    pub(crate) fn post_s20_bench_cooldown(
        &mut self,
        snapshot: &DbSnapshot,
        date: NaiveDate,
    ) -> Result<()> {
        if self.cfg.bench_cooldown_penalty == 0 {
            return Ok(());
        }
        let mut count = 0usize;
        let ModelBuilder {
            solver,
            available,
            x,
            obj_terms,
            obj_term_evals,
            cfg,
            ..
        } = self;

        const BENCH_COOLDOWN_DAYS: i64 = 7;

        for (r_idx, rower) in available.iter().enumerate() {
            if rower.is_designated_cox.as_bool() {
                continue;
            }
            let Some(last_date) = snapshot.last_benched.get(&rower.id) else {
                continue;
            };
            let days_since = (date - *last_date).num_days();
            if days_since < 0 || days_since >= BENCH_COOLDOWN_DAYS {
                continue;
            }

            // Linear decay: recently benched = full penalty, older = less.
            let numerator = cfg.bench_cooldown_penalty as i64 * (BENCH_COOLDOWN_DAYS - days_since);
            let effective = ((numerator + BENCH_COOLDOWN_DAYS - 1) / BENCH_COOLDOWN_DAYS) as i32;
            if effective <= 0 {
                continue;
            }

            let rower_vars: Vec<DomainId> = x
                .iter()
                .filter_map(|(&(r, _, _), &v)| if r == r_idx { Some(v) } else { None })
                .collect();
            if rower_vars.is_empty() {
                continue;
            }

            let rower_used = solver.new_bounded_integer(0, 1);
            let mut link: Vec<AffineView<DomainId>> =
                rower_vars.iter().map(|v| v.scaled(1)).collect();
            link.push(rower_used.scaled(-1));
            let tag = solver.new_constraint_tag();
            solver
                .add_constraint(pumpkin_constraints::equals(link, 0, tag))
                .post()
                .map_err(|e| anyhow!("S20 bench-cooldown link: {e:?}"))?;

            // Reward placing = penalise benching again.
            push_obj!(obj_terms, obj_term_evals, rower_used, -effective);
            count += 1;
        }
        tracing::debug!(terms = count, "S20 bench cooldown");
        Ok(())
    }

    /// **S21 — stroke spread.** Penalise placing multiple "designated
    /// strokes" (rowers with a Stroke zone affinity of weight >= 2) in
    /// the same boat. For each pair of designated strokes (i, j), a
    /// "together" indicator is reified per boat — if both are placed in
    /// the same boat the penalty is `weight`. With N strokes in one
    /// boat the total penalty is `weight * C(N,2)` = 0, 0, 1w, 3w, 6w.
    pub(crate) fn post_s21_stroke_spread(
        &mut self,
        snapshot: &lineup_db::snapshot::DbSnapshot,
    ) -> Result<()> {
        if self.cfg.stroke_spread_weight == 0 {
            return Ok(());
        }
        let mut count = 0usize;
        let ModelBuilder {
            solver,
            boats,
            available,
            x,
            obj_terms,
            obj_term_evals,
            cfg,
            ..
        } = self;

        // Identify designated strokes: rowers with Stroke zone affinity >= 2.
        let stroke_rower_indices: Vec<usize> = {
            use lineup_db::seat_affinity::SeatZone;
            use std::collections::HashSet;

            let stroke_rower_ids: HashSet<lineup_db::rower::types::RowerId> = snapshot
                .seat_affinities
                .iter()
                .filter(|a| a.zone == SeatZone::Stroke && a.weight.as_int() >= 2)
                .map(|a| a.rower_id)
                .collect();

            available
                .iter()
                .enumerate()
                .filter(|(_, r)| stroke_rower_ids.contains(&r.id))
                .map(|(i, _)| i)
                .collect()
        };

        if stroke_rower_indices.len() <= 1 {
            return Ok(()); // 0 or 1 designated stroke — no spreading needed
        }

        // For each pair of designated strokes, for each boat, penalize
        // both being placed in that boat.
        for i in 0..stroke_rower_indices.len() {
            for j in (i + 1)..stroke_rower_indices.len() {
                let r_a = stroke_rower_indices[i];
                let r_b = stroke_rower_indices[j];

                for (b_idx, boat) in boats.iter().enumerate() {
                    // Collect x vars for rower A in this boat (any seat).
                    let a_vars: Vec<DomainId> = (0..=boat.seat_count.as_int())
                        .filter(|&s| s == 0 && boat.has_cox.as_bool() || s >= 1)
                        .filter_map(|s| x.get(&(r_a, b_idx, s)).copied())
                        .collect();
                    let b_vars: Vec<DomainId> = (0..=boat.seat_count.as_int())
                        .filter(|&s| s == 0 && boat.has_cox.as_bool() || s >= 1)
                        .filter_map(|s| x.get(&(r_b, b_idx, s)).copied())
                        .collect();

                    if a_vars.is_empty() || b_vars.is_empty() {
                        continue;
                    }

                    // a_in_boat = Σ a_vars (0 or 1)
                    let a_in = solver.new_bounded_integer(0, 1);
                    let mut link_a: Vec<AffineView<DomainId>> =
                        a_vars.iter().map(|v| v.scaled(1)).collect();
                    link_a.push(a_in.scaled(-1));
                    let tag = solver.new_constraint_tag();
                    solver
                        .add_constraint(pumpkin_constraints::equals(link_a, 0, tag))
                        .post()
                        .map_err(|e| anyhow!("S21 stroke-spread a_in link: {e:?}"))?;

                    // b_in_boat = Σ b_vars (0 or 1)
                    let b_in = solver.new_bounded_integer(0, 1);
                    let mut link_b: Vec<AffineView<DomainId>> =
                        b_vars.iter().map(|v| v.scaled(1)).collect();
                    link_b.push(b_in.scaled(-1));
                    let tag = solver.new_constraint_tag();
                    solver
                        .add_constraint(pumpkin_constraints::equals(link_b, 0, tag))
                        .post()
                        .map_err(|e| anyhow!("S21 stroke-spread b_in link: {e:?}"))?;

                    // together = min(a_in, b_in): 1 iff both are in this boat
                    let together = solver.new_bounded_integer(0, 1);
                    let tag = solver.new_constraint_tag();
                    solver
                        .add_constraint(pumpkin_constraints::minimum(
                            vec![a_in, b_in],
                            together,
                            tag,
                        ))
                        .post()
                        .map_err(|e| anyhow!("S21 stroke-spread together: {e:?}"))?;

                    push_obj!(
                        obj_terms,
                        obj_term_evals,
                        together,
                        cfg.stroke_spread_weight
                    );
                    count += 1;
                }
            }
        }
        tracing::debug!(terms = count, "S21 stroke spread");
        Ok(())
    }
}
