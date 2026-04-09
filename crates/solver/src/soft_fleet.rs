//! Fleet-level soft constraints: S4 wrong-side aggregation, S6 cox
//! cooldown, S7 novelty vs recent lineups, and S8 placement reward.
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

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use lineup_db::boat::types::BoatId;
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
    /// the rolling `COX_COOLDOWN_DAYS` window incur a flat penalty
    /// if the solver tries to seat them as cox again. Designated
    /// coxes are exempt.
    ///
    /// Encoding mirrors S4's per-rower aggregation: gather the
    /// rower's cox-seat x vars across all coxed candidate boats
    /// (coxless boats don't create a seat-0 x), sum them into a
    /// `cox_use[r] ∈ {0, 1}` aux var via a link equality (by H2 the
    /// sum is ≤ 1), and push exactly one obj term per penalised
    /// rower. Cost: `O(rowers in cooldown)`.
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
        Ok(())
    }

    /// S7 — novelty vs recently-committed lineups. Penalises
    /// assignments that are too similar to a historical lineup
    /// from `snapshot.recent_placements`.
    ///
    /// Per historical lineup `L` with `N_L` still-reachable
    /// (rower, boat, seat) placements:
    ///
    ///   `threshold = N_L − novelty_factor − 1`
    ///   `match_L   = Σ x[r, b, s]` over L's reachable placements
    ///   `penalty_L ≥ match_L − threshold`     (posted as inequality)
    ///   `penalty_L ≥ 0`                        (via domain)
    ///   `obj_terms.push(penalty_L.scaled(novelty_weight))`
    ///
    /// Since the solver minimises, `penalty_L` ends up as
    /// `max(0, match_L − threshold)` — zero below the threshold,
    /// linearly growing above it. Cox seats are excluded because
    /// cox rotation is governed by S6 cooldown, which would
    /// otherwise fight against this constraint's designated-exempt
    /// case.
    ///
    /// Cost: one aux var + one inequality per historical lineup.
    /// With the default recent-lineup window and a realistic
    /// fleet, that's at most ~16 constraints — tiny.
    pub(crate) fn post_s7_novelty(
        &mut self,
        snapshot: &DbSnapshot,
        novelty_factor: i32,
    ) -> Result<()> {
        if novelty_factor <= 0 || self.cfg.novelty_weight == 0 {
            return Ok(());
        }

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

        let ModelBuilder {
            solver,
            boats,
            available,
            x,
            obj_terms,
            cfg,
            ..
        } = self;
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

            let threshold = reachable_matches - novelty_factor - 1;
            // If the threshold is ≥ max possible match count, the
            // constraint is trivially slack (penalty always 0) — skip
            // posting it to save Pumpkin work.
            if threshold >= reachable_matches {
                continue;
            }

            // penalty upper bound: max possible is `reachable_matches
            // - threshold` = `factor + 1`. Overshoot slightly for
            // safety.
            let penalty_upper = novelty_factor + 2;
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
        Ok(())
    }
}
