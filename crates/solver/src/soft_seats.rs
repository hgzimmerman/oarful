//! Seat-level soft constraints: the shared per-seat trait map
//! prelude, plus S1 (skill variance), S2 (pair affinities), S3
//! (seat affinities), S9 + S9b (pair strength balance, with bow-
//! pair emphasis), S10 (pair height balance), S11 (8-boat end-pair
//! skill reward), and S12 (8-boat engine-room strength reward).
//!
//! "Seat-level" here means "reasons about the per-seat trait
//! aggregate vars (`seat_skill[b, s]`, `seat_strength[b, s]`,
//! `seat_height[b, s]`) or about per-partition pair
//! considerations". The S2 pair-affinity and S3 seat-affinity
//! blocks are included because they also operate at the seat
//! granularity, even though they bypass the trait maps and go
//! straight to the `x[r, b, s]` vars.
//!
//! Fleet-level softs (S4 wrong-side, S6 cox cooldown, S7 novelty,
//! S8 placement reward) live in `soft_fleet.rs`.

use anyhow::{anyhow, Result};
use lineup_db::snapshot::DbSnapshot;
use pumpkin_core::variables::{DomainId, TransformableVariable};

use crate::model::{build_seat_trait_map, ModelBuilder};

impl<'a> ModelBuilder<'a> {
    /// Build the shared `seat_skill_by_seat`, `seat_strength_by_seat`,
    /// and `seat_height_by_seat` maps — but only for the traits
    /// whose consumer soft constraints are enabled in `cfg`.
    ///
    /// Gating avoids allocating aux vars + link equalities for trait
    /// maps no downstream constraint will read. See
    /// `build_seat_trait_map` in `model.rs` for the per-trait
    /// construction details.
    pub(crate) fn build_seat_trait_maps(&mut self) -> Result<()> {
        if self.cfg.skill_variance_weight != 0 || self.cfg.end_pair_skill_weight != 0 {
            self.seat_skill_by_seat = build_seat_trait_map(
                &mut self.solver,
                &self.boats,
                &self.available,
                &self.x,
                |r| r.skill.ordinal(),
                "seat skill link",
            )?;
        }

        if self.cfg.pair_strength_weight != 0
            || self.cfg.engine_room_strength_weight != 0
        {
            self.seat_strength_by_seat = build_seat_trait_map(
                &mut self.solver,
                &self.boats,
                &self.available,
                &self.x,
                |r| r.strength.ordinal(),
                "seat strength link",
            )?;
        }

        if self.cfg.height_balance_weight != 0 {
            self.seat_height_by_seat = build_seat_trait_map(
                &mut self.solver,
                &self.boats,
                &self.available,
                &self.x,
                |r| r.height.ordinal(),
                "seat height link",
            )?;
        }
        Ok(())
    }

    /// S1 — skill variance within each boat. Penalises large
    /// `max − min` spread in skill ordinals across the required
    /// rowing seats, using Pumpkin's `maximum` / `minimum` global
    /// constraints over the shared `seat_skill_by_seat` map.
    ///
    /// Each unit of spread contributes `skill_variance_weight` to
    /// the objective. Optional (partial-fill) seats are excluded
    /// by the shared map, so spread reflects the "stable core" of
    /// the boat even under non-strict partial-fill policies.
    pub(crate) fn post_s1_skill_variance(&mut self) -> Result<()> {
        if self.cfg.skill_variance_weight == 0 {
            return Ok(());
        }
        let ModelBuilder {
            solver,
            boats,
            obj_terms,
            seat_skill_by_seat,
            cfg,
            ..
        } = self;
        for (b_idx, boat) in boats.iter().enumerate() {
            let n_rowing = boat.seat_count;
            if n_rowing == 0 {
                continue;
            }

            // The shared `seat_skill_by_seat` map already excludes
            // optional (partial-fill) seats. Collect this boat's
            // required rowing seats in order — max/min operates on
            // values, so seat order doesn't matter beyond stability
            // for debugging.
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
        Ok(())
    }

    /// S2 — pair affinities from the `pair_affinity` table. For
    /// each stored `(A, B, w)` and each candidate boat, iterates
    /// the boat's 2-seat partitions and introduces a reified
    /// boolean `together[pair, boat, partition] ∈ {0, 1}` driven
    /// by the AND of "A is in this partition" and "B is in this
    /// partition":
    ///
    /// ```text
    /// A_in_part := x[A, b, s_lo] + x[A, b, s_hi]   (at most 1 by H2)
    /// B_in_part := x[B, b, s_lo] + x[B, b, s_hi]   (at most 1)
    ///
    /// together ≤ A_in_part
    /// together ≤ B_in_part
    /// together ≥ A_in_part + B_in_part − 1
    /// ```
    ///
    /// Three linear inequalities per (stored_pair × boat ×
    /// partition). Then `together.scaled(-w)` pushes into
    /// `obj_terms`. Positive `w` rewards pair-sharing; negative
    /// `w` penalises it.
    ///
    /// Unavailable rowers, designated coxes, and structurally-
    /// incompatible partitions all yield inert terms rather than
    /// errors — see the README §S2 "naturally inert cases" notes.
    pub(crate) fn post_s2_pair_affinities(
        &mut self,
        snapshot: &DbSnapshot,
    ) -> Result<()> {
        if self.cfg.pair_affinity_weight == 0 {
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

                    obj_terms.push(
                        together.scaled(-aff.weight.as_int() * cfg.pair_affinity_weight),
                    );

                    s_lo += 2;
                }
            }
        }
        Ok(())
    }

    /// S3 — seat affinities from the `rower_seat_affinity` table.
    /// For each stored `(rower, seat_position, weight)` entry,
    /// pushes `x[r, b, seat_position].scaled(-weight)` into
    /// `obj_terms` for every candidate boat that has a matching
    /// seat position. Positive weights become negative obj
    /// contributions (rewards); negative weights become penalties.
    ///
    /// `seat_position` is boat-agnostic: a preference for seat 8
    /// only applies to 8-boats, and a preference for seat 4
    /// applies to 4-boats *and* to seat 4 of 8-boats.
    pub(crate) fn post_s3_seat_affinities(
        &mut self,
        snapshot: &DbSnapshot,
    ) -> Result<()> {
        if self.cfg.seat_affinity_weight == 0 {
            return Ok(());
        }
        let ModelBuilder {
            boats,
            available,
            x,
            obj_terms,
            cfg,
            ..
        } = self;
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
                    obj_terms.push(
                        var.scaled(-aff.weight.as_int() * cfg.seat_affinity_weight),
                    );
                }
            }
        }
        Ok(())
    }

    /// S9 + S9b — pair strength balance (with a layered
    /// bow-pair emphasis).
    ///
    /// Within a single 2-seat partition the two rowers should have
    /// similar strength ordinals; a mismatch means one side pulls
    /// harder than the other and the hull yaws off course.
    /// Encoded per partition via Pumpkin's `maximum` / `minimum`
    /// over the two `seat_strength_by_seat` vars, then a linear
    /// equality `pair_max − pair_min = diff`, then
    /// `obj_terms.push(diff.scaled(pair_strength_weight))`.
    ///
    /// S9b layers an additional
    /// `diff.scaled(bow_pair_strength_weight)` term on top of the
    /// bow partition `(1, 2)` only, because bow pair influences
    /// set and steering more than any other partition. Safe to
    /// push the same `diff` DomainId twice — `AffineView` is a
    /// Copy projection and the objective-link equality just
    /// accumulates both coefficients.
    pub(crate) fn post_s9_pair_strength(&mut self) -> Result<()> {
        if self.cfg.pair_strength_weight == 0 {
            return Ok(());
        }
        let ModelBuilder {
            solver,
            boats,
            obj_terms,
            seat_strength_by_seat,
            cfg,
            ..
        } = self;
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
                if s_lo == 1 && cfg.bow_pair_strength_weight != 0 {
                    obj_terms.push(diff.scaled(cfg.bow_pair_strength_weight));
                }

                s_lo += 2;
            }
        }
        Ok(())
    }

    /// S10 — pair height balance. Structurally identical to S9 but
    /// operates on the `seat_height_by_seat` map and contributes
    /// `diff.scaled(height_balance_weight)` per partition. Gentle
    /// preference rather than a hard rule — mixed-height pairs row
    /// fine, this is just a nudge to match oar-handle heights and
    /// catch timing.
    pub(crate) fn post_s10_pair_height(&mut self) -> Result<()> {
        if self.cfg.height_balance_weight == 0 {
            return Ok(());
        }
        let ModelBuilder {
            solver,
            boats,
            obj_terms,
            seat_height_by_seat,
            cfg,
            ..
        } = self;
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
        Ok(())
    }

    /// S11 — end-pair skill reward (8-boats only). Rewards placing
    /// high-skill rowers in seats {1, 2, 7, 8} of an eight by
    /// pushing `seat_skill[b, s].scaled(-end_pair_skill_weight)`
    /// for each end-pair seat. The objective is minimised, so a
    /// negative-coefficient term on `seat_skill` effectively
    /// maximises it.
    ///
    /// Unused boats have all `x = 0`, forcing `seat_skill = 0`, so
    /// the reward contributes nothing — no phantom "bench a boat"
    /// incentive. Piggybacks on the shared `seat_skill_by_seat`
    /// map alongside S1.
    pub(crate) fn post_s11_end_pair_skill(&mut self) {
        if self.cfg.end_pair_skill_weight == 0 {
            return;
        }
        const END_PAIR_SEATS: [i32; 4] = [1, 2, 7, 8];
        for (b_idx, boat) in self.boats.iter().enumerate() {
            if boat.seat_count != 8 {
                continue;
            }
            for seat in END_PAIR_SEATS {
                if let Some(&s_var) = self.seat_skill_by_seat.get(&(b_idx, seat)) {
                    self.obj_terms
                        .push(s_var.scaled(-self.cfg.end_pair_skill_weight));
                }
            }
        }
    }

    /// S12 — engine-room strength reward (8-boats only).
    /// Structurally identical to S11 but over the
    /// `seat_strength_by_seat` map and the engine-room seat set
    /// {3, 4, 5, 6}. Rewards placing the strongest rowers in the
    /// middle four seats of an eight, where raw propulsive force
    /// matters more than technical skill.
    pub(crate) fn post_s12_engine_room_strength(&mut self) {
        if self.cfg.engine_room_strength_weight == 0 {
            return;
        }
        const ENGINE_ROOM_SEATS: [i32; 4] = [3, 4, 5, 6];
        for (b_idx, boat) in self.boats.iter().enumerate() {
            if boat.seat_count != 8 {
                continue;
            }
            for seat in ENGINE_ROOM_SEATS {
                if let Some(&s_var) = self.seat_strength_by_seat.get(&(b_idx, seat))
                {
                    self.obj_terms
                        .push(s_var.scaled(-self.cfg.engine_room_strength_weight));
                }
            }
        }
    }
}
