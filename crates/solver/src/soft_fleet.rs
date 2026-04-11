//! Fleet-level soft constraints: S4 wrong-side aggregation, S6 cox
//! cooldown, S8 placement reward, S13 non-scull retention, and
//! reference-lineup similarity (unified novelty / baseline).
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
        for (b_idx, boat) in self.boats.iter().enumerate() {
            let seats_total =
                boat.seat_count + if boat.has_cox.as_bool() { 1 } else { 0 };
            let coef = -seats_total * self.cfg.placement_reward_weight;
            if coef != 0 {
                self.obj_terms.push(self.use_b[b_idx].scaled(coef));
            }
        }
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
        let ModelBuilder {
            solver,
            available,
            obj_terms,
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

            obj_terms.push(wrong_count.scaled(coef));
        }
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
        let ModelBuilder {
            solver,
            boats,
            available,
            x,
            obj_terms,
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
            let numerator = cfg.cox_cooldown_penalty as i64
                * (COX_COOLDOWN_DAYS - days_since);
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

            obj_terms.push(cox_use.scaled(effective));
        }
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
            ..
        } = self;
        for reference in references {
            if reference.weight == 0 {
                continue;
            }
            for p in &reference.placements {
                let Some(r_idx) = available.iter().position(|r| r.id == p.rower_id)
                else {
                    continue;
                };
                let Some(b_idx) = boats.iter().position(|b| b.id == p.boat_id) else {
                    continue;
                };
                if let Some(&var) = x.get(&(r_idx, b_idx, p.seat)) {
                    obj_terms.push(var.scaled(reference.weight));
                }
            }
        }
        Ok(())
    }

    /// S13 — non-scull retention. Rewards placing rowers who
    /// *can't* fall back to the scullers team, so the solver
    /// prefers benching sculling-eligible rowers over sweep-only
    /// ones when it has to bench somebody.
    ///
    /// Without this term, the S8 placement reward treats every
    /// rower the same — rewarding "seat filled" without caring
    /// who's in it. Two equivalent assignments, one benching a
    /// `can_scull = true` rower and one benching a `can_scull =
    /// false` rower, look identical to the objective; the
    /// solver picks arbitrarily. That's wrong from the coach's
    /// perspective: the can_scull rower has a sensible fallback
    /// (go row singles today), whereas the sweep-only rower
    /// sits on the dock.
    ///
    /// Encoding mirrors S4 / S6's per-rower aggregation — one
    /// `rower_used[r] ∈ {0, 1}` aux var per *non-scull*
    /// available rower, linked to `Σ_{b, s} x[r, b, s]` by an
    /// equality. By H2 the sum is at most 1, so the `[0, 1]`
    /// domain is tight. Push exactly one obj term per
    /// penalised rower:
    ///
    ///   `obj_terms.push(rower_used[r].scaled(-retention_weight))`
    ///
    /// Total contribution is O(non-scull rowers) obj terms. When
    /// rower r is placed anywhere, `rower_used[r] = 1` and the
    /// objective drops by `retention_weight`. When r is
    /// benched, `rower_used[r] = 0` and no reward accrues.
    ///
    /// Rowers with `can_scull = true` are deliberately skipped —
    /// the baseline S8 reward already covers the "please place
    /// them if convenient" case, and adding the retention
    /// bonus uniformly would just shift the constant and not
    /// break any ties.
    pub(crate) fn post_s13_non_scull_retention(&mut self) -> Result<()> {
        if self.cfg.non_scull_retention_weight == 0 {
            return Ok(());
        }
        let ModelBuilder {
            solver,
            available,
            x,
            obj_terms,
            cfg,
            ..
        } = self;
        for (r_idx, rower) in available.iter().enumerate() {
            if rower.can_scull.as_bool() {
                // Sculling-eligible rowers fall back to the
                // scullers team if benched — no retention pressure.
                continue;
            }
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

            obj_terms.push(rower_used.scaled(-cfg.non_scull_retention_weight));
        }
        Ok(())
    }

    /// S14 — bow-loader cox fit penalty. Penalises tall and heavy
    /// rowers in the cox seat (position 0) of bow-loader boats.
    /// Height is the primary factor; weight is secondary.
    ///
    /// Penalty table (multiplied by `bow_cox_fit_weight`):
    /// - Tall: 5, Very tall: 8
    /// - Heavy: 1, (very heavy rowers don't exist in the current
    ///   `RowerWeightClass` enum, but if added: 3)
    ///
    /// Only applies when `boat.cox_position == CoxPosition::Bow`.
    /// Stern-loader cox seats have no size constraint.
    pub(crate) fn post_s14_bow_cox_fit(&mut self) -> Result<()> {
        use lineup_db::boat::types::CoxPosition;
        use lineup_db::rower::types::Height;

        if self.cfg.bow_cox_fit_weight == 0 {
            return Ok(());
        }
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
                    Height::Tall => 5,
                    Height::VeryTall => 8,
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
                self.obj_terms.push(var.scaled(scaled));
            }
        }
        Ok(())
    }
}
