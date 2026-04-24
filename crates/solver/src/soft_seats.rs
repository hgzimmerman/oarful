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
use lineup_db::seat_affinity::SeatZone;
use lineup_db::snapshot::DbSnapshot;
use pumpkin_core::variables::{DomainId, TransformableVariable};

use crate::model::{build_seat_trait_map, ModelBuilder};
use crate::PartialFillPolicy;

impl<'a> ModelBuilder<'a> {
    /// Build the shared `seat_skill_by_seat`, `seat_strength_by_seat`,
    /// and `seat_height_by_seat` maps — but only for the traits
    /// whose consumer soft constraints are enabled in `cfg`.
    ///
    /// `partial_fill` controls whether optional (partial-fill) seats
    /// are included in the maps. Under `Strict` every seat is
    /// required and optional seats are included, letting S9 pair
    /// diffs and S12 engine-room rewards see partition (3, 4) of
    /// an 8+. Under `Allowed`, optional seats are skipped so a
    /// legitimately empty seat can't poison max/min/spread
    /// calculations.
    ///
    /// Gating on config weights avoids allocating aux vars + link
    /// equalities for trait maps no downstream constraint will read.
    /// See `build_seat_trait_map` in `model.rs` for the per-trait
    /// construction details.
    pub(crate) fn build_seat_trait_maps(&mut self, partial_fill: PartialFillPolicy) -> Result<()> {
        if self.cfg.skill_variance_weight != 0
            || self.cfg.end_pair_skill_weight != 0
            || self.cfg.pair_strength_weight != 0
        {
            self.seat_skill_by_seat = build_seat_trait_map(
                &mut self.solver,
                &self.boats,
                &self.available,
                &self.x,
                partial_fill,
                |r| r.skill.ordinal(),
                "seat skill link",
            )?;
        }

        if self.cfg.pair_strength_weight != 0 || self.cfg.engine_room_strength_weight != 0 {
            self.seat_strength_by_seat = build_seat_trait_map(
                &mut self.solver,
                &self.boats,
                &self.available,
                &self.x,
                partial_fill,
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
                partial_fill,
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
        let mut count = 0usize;
        let ModelBuilder {
            solver,
            boats,
            obj_terms,
            seat_skill_by_seat,
            cfg,
            ..
        } = self;
        for (b_idx, boat) in boats.iter().enumerate() {
            let n_rowing = boat.seat_count.as_int();
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
            count += 1;
        }
        tracing::debug!(terms = count, "S1 skill variance");
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
    pub(crate) fn post_s2_pair_affinities(&mut self, snapshot: &DbSnapshot) -> Result<()> {
        if self.cfg.pair_affinity_weight == 0 {
            return Ok(());
        }
        let mut count = 0usize;
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
                while s_lo + 1 <= boat.seat_count.as_int() {
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
                        .add_constraint(pumpkin_constraints::less_than_or_equals(upper_a, 0, tag))
                        .post()
                        .map_err(|e| anyhow!("pair reif upper-A: {e:?}"))?;

                    // together ≤ B_in_part
                    let mut upper_b = vec![together.scaled(1)];
                    for t in &b_terms {
                        upper_b.push(t.scaled(-1));
                    }
                    let tag = solver.new_constraint_tag();
                    solver
                        .add_constraint(pumpkin_constraints::less_than_or_equals(upper_b, 0, tag))
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
                        .add_constraint(pumpkin_constraints::less_than_or_equals(lower, 1, tag))
                        .post()
                        .map_err(|e| anyhow!("pair reif lower: {e:?}"))?;

                    obj_terms
                        .push(together.scaled(-aff.weight.as_int() * cfg.pair_affinity_weight));
                    count += 1;

                    s_lo += 2;
                }
            }
        }
        tracing::debug!(terms = count, "S2 pair affinities");
        Ok(())
    }

    /// S3 — zone-based seat affinities from `rower_seat_affinity`.
    ///
    /// Each affinity specifies a zone (Stroke, Engine Room, etc.)
    /// which maps to concrete seats based on boat size. When multiple
    /// zones overlap on the same (rower, boat, seat), we take the MAX
    /// weight to avoid double-counting.
    pub(crate) fn post_s3_seat_affinities(&mut self, snapshot: &DbSnapshot) -> Result<()> {
        if self.cfg.seat_affinity_weight == 0 {
            return Ok(());
        }
        let mut count = 0usize;
        let ModelBuilder {
            boats,
            available,
            x,
            obj_terms,
            cfg,
            ..
        } = self;

        // Build (rower_idx, boat_idx, seat) → effective weight across
        // all matching zones. This deduplicates overlapping zones so a
        // rower with Stroke(+5) and SternPair(+3) on seat 8 gets +5,
        // not +8. BTreeMap keeps iteration order deterministic so the
        // solver sees terms in a stable order.
        //
        // Single-seat zones (Stroke, Bow) get a 2× boost because the
        // rower must land in one exact seat — multi-seat zones spread
        // the reward across several seats, giving the solver natural
        // flexibility that single-seat zones lack.
        let mut best: std::collections::BTreeMap<(usize, usize, i32), i32> =
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
            let r_idx = match available.iter().position(|r| r.id == aff.rower_id) {
                Some(i) => i,
                None => continue,
            };
            for (b_idx, boat) in boats.iter().enumerate() {
                for seat in aff.zone.seats_for(boat.seat_count.as_int()) {
                    let key = (r_idx, b_idx, seat);
                    best.entry(key)
                        .and_modify(|w| *w = (*w).max(effective))
                        .or_insert(effective);
                }
            }
        }

        for ((r_idx, b_idx, seat), weight) in &best {
            if let Some(&var) = x.get(&(*r_idx, *b_idx, *seat)) {
                obj_terms.push(var.scaled(-weight * cfg.seat_affinity_weight));
                count += 1;
            }
        }
        tracing::debug!(terms = count, "S3 seat affinities");
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
    /// S9 — pair mismatch penalty. For each 2-seat partition,
    /// penalizes the combined strength and skill difference between
    /// the two rowers, weighted 2:1 (strength mismatches matter more
    /// than skill mismatches for boat balance).
    ///
    /// `penalty = strength_diff * 2 + skill_diff`
    ///
    /// S9b layers an additional penalty on the bow partition (seats
    /// 1, 2) because bow pair influences set and steering more than
    /// any other partition.
    pub(crate) fn post_s9_pair_strength(&mut self) -> Result<()> {
        if self.cfg.pair_strength_weight == 0 {
            return Ok(());
        }
        let mut count = 0usize;
        let ModelBuilder {
            solver,
            boats,
            obj_terms,
            seat_strength_by_seat,
            seat_skill_by_seat,
            cfg,
            ..
        } = self;
        for (b_idx, boat) in boats.iter().enumerate() {
            let n_rowing = boat.seat_count.as_int();
            if n_rowing < 2 {
                continue;
            }

            let mut s_lo = 1i32;
            while s_lo + 1 <= n_rowing {
                let s_hi = s_lo + 1;

                // Strength diff (weight 2)
                let str_diff = match (
                    seat_strength_by_seat.get(&(b_idx, s_lo)).copied(),
                    seat_strength_by_seat.get(&(b_idx, s_hi)).copied(),
                ) {
                    (Some(lo), Some(hi)) => {
                        let mx = solver.new_bounded_integer(0, 4);
                        let mn = solver.new_bounded_integer(0, 4);
                        let tag = solver.new_constraint_tag();
                        solver
                            .add_constraint(pumpkin_constraints::maximum(vec![lo, hi], mx, tag))
                            .post()
                            .map_err(|e| anyhow!("pair str max: {e:?}"))?;
                        let tag = solver.new_constraint_tag();
                        solver
                            .add_constraint(pumpkin_constraints::minimum(vec![lo, hi], mn, tag))
                            .post()
                            .map_err(|e| anyhow!("pair str min: {e:?}"))?;
                        let d = solver.new_bounded_integer(0, 3);
                        let tag = solver.new_constraint_tag();
                        solver
                            .add_constraint(pumpkin_constraints::equals(
                                vec![mx.scaled(1), mn.scaled(-1), d.scaled(-1)],
                                0,
                                tag,
                            ))
                            .post()
                            .map_err(|e| anyhow!("pair str diff: {e:?}"))?;
                        Some(d)
                    }
                    _ => None,
                };

                // Skill diff (weight 1)
                let skill_diff = match (
                    seat_skill_by_seat.get(&(b_idx, s_lo)).copied(),
                    seat_skill_by_seat.get(&(b_idx, s_hi)).copied(),
                ) {
                    (Some(lo), Some(hi)) => {
                        let mx = solver.new_bounded_integer(0, 4);
                        let mn = solver.new_bounded_integer(0, 4);
                        let tag = solver.new_constraint_tag();
                        solver
                            .add_constraint(pumpkin_constraints::maximum(vec![lo, hi], mx, tag))
                            .post()
                            .map_err(|e| anyhow!("pair skill max: {e:?}"))?;
                        let tag = solver.new_constraint_tag();
                        solver
                            .add_constraint(pumpkin_constraints::minimum(vec![lo, hi], mn, tag))
                            .post()
                            .map_err(|e| anyhow!("pair skill min: {e:?}"))?;
                        let d = solver.new_bounded_integer(0, 3);
                        let tag = solver.new_constraint_tag();
                        solver
                            .add_constraint(pumpkin_constraints::equals(
                                vec![mx.scaled(1), mn.scaled(-1), d.scaled(-1)],
                                0,
                                tag,
                            ))
                            .post()
                            .map_err(|e| anyhow!("pair skill diff: {e:?}"))?;
                        Some(d)
                    }
                    _ => None,
                };

                // Skip partition if neither diff could be computed
                // (optional seats under partial fill).
                if str_diff.is_none() && skill_diff.is_none() {
                    s_lo += 2;
                    continue;
                }

                // Combined penalty: strength_diff * 2 + skill_diff
                if let Some(sd) = str_diff {
                    obj_terms.push(sd.scaled(cfg.pair_strength_weight * 2));
                    count += 1;
                }
                if let Some(kd) = skill_diff {
                    obj_terms.push(kd.scaled(cfg.pair_strength_weight));
                    count += 1;
                }

                // S9b: bow pair extra penalty
                if s_lo == 1 && cfg.bow_pair_strength_weight != 0 {
                    if let Some(sd) = str_diff {
                        obj_terms.push(sd.scaled(cfg.bow_pair_strength_weight * 2));
                        count += 1;
                    }
                    if let Some(kd) = skill_diff {
                        obj_terms.push(kd.scaled(cfg.bow_pair_strength_weight));
                        count += 1;
                    }
                }

                s_lo += 2;
            }
        }
        tracing::debug!(terms = count, "S9 pair mismatch");
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
        let mut count = 0usize;
        let ModelBuilder {
            solver,
            boats,
            obj_terms,
            seat_height_by_seat,
            cfg,
            ..
        } = self;
        for (b_idx, boat) in boats.iter().enumerate() {
            let n_rowing = boat.seat_count.as_int();
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
                count += 1;

                s_lo += 2;
            }
        }
        tracing::debug!(terms = count, "S10 pair height");
        Ok(())
    }

    /// S11 — skill gradient across all seats. Full reward on bow
    /// pair and stern pair zones, with a gradient into the engine
    /// room that tapers toward the outer seats. This creates a
    /// skill ordering within the engine room (5/6 attract more
    /// skilled rowers than 3/4) without treating all engine room
    /// seats as interchangeable.
    ///
    /// Coefficients by distance from nearest end (bow or stroke):
    /// - Distance 0 (end pair seats): `end_pair_skill_weight`
    /// - Distance 1 (one inward):     `max(1, weight * 3/4)`
    /// - Distance 2 (two inward):     `max(1, weight * 1/2)`
    /// - Distance 3+ (deep interior): `max(1, weight * 1/4)`
    ///
    /// The `max(1, …)` floor ensures the gradient exists even at
    /// low config weights — the interior seats always get at least
    /// 1 unit of skill reward when S11 is enabled.
    ///
    /// For an 8+: s1,s2,s7,s8 → full; s3,s6 → 3/4; s4,s5 → 1/2.
    /// For a 4+: all seats are distance 0 or 1 → full or 3/4.
    /// For pairs/singles: no seats (N < 2 skipped).
    ///
    /// Unused boats have all `x = 0`, forcing `seat_skill = 0`, so
    /// the reward contributes nothing. Piggybacks on the shared
    /// `seat_skill_by_seat` map alongside S1.
    pub(crate) fn post_s11_end_pair_skill(&mut self) {
        if self.cfg.end_pair_skill_weight == 0 {
            return;
        }
        let mut count = 0usize;
        let w = self.cfg.end_pair_skill_weight;
        for (b_idx, boat) in self.boats.iter().enumerate() {
            let n = boat.seat_count.as_int();
            if n < 2 {
                continue;
            }
            for seat in 1..=n {
                // Distance from the nearest end (bow=1 or stroke=N).
                let dist = (seat - 1).min(n - seat);
                let coef = match dist {
                    0 => w,
                    1 => (w * 3 / 4).max(1),
                    2 => (w / 2).max(1),
                    _ => (w / 4).max(1),
                };
                if let Some(&s_var) = self.seat_skill_by_seat.get(&(b_idx, seat)) {
                    self.obj_terms.push(s_var.scaled(-coef));
                    count += 1;
                }
            }
        }
        tracing::debug!(terms = count, "S11 end-pair skill");
    }

    /// S12 — engine-room strength reward. Rewards placing the
    /// strongest rowers in the engine room zone, where raw
    /// propulsive force matters more than technical skill. The zone
    /// mapping generalises across boat sizes (seats {3,4,5,6} in
    /// an 8+, {2,3} in a 4+, empty in pairs/singles).
    pub(crate) fn post_s12_engine_room_strength(&mut self) {
        if self.cfg.engine_room_strength_weight == 0 {
            return;
        }
        let mut count = 0usize;
        for (b_idx, boat) in self.boats.iter().enumerate() {
            for seat in SeatZone::EngineRoom.seats_for(boat.seat_count.as_int()) {
                if let Some(&s_var) = self.seat_strength_by_seat.get(&(b_idx, seat)) {
                    self.obj_terms
                        .push(s_var.scaled(-self.cfg.engine_room_strength_weight));
                    count += 1;
                }
            }
        }
        tracing::debug!(terms = count, "S12 engine-room strength");
    }

    /// S16 — top-boat stacking bonus. Gives the first boat (b_idx=0)
    /// an extra per-seat skill + strength reward so the solver
    /// concentrates the best rowers there. Without this, S11/S12
    /// fire equally for all boats and the solver distributes talent
    /// evenly. Only meaningful when `top_boat_stacking_weight > 0`
    /// (the tiered preset enables it, balanced does not).
    /// S16 — top-boat stacking bonus. Works directly on x variables
    /// (not trait maps) so it sees all seats including optional ones
    /// under partial fill. For each eligible (rower, boat_0, seat),
    /// pushes `-weight * (skill_ordinal + strength_ordinal)` scaled
    /// by the x variable.
    /// S16 — boat stacking differentiation. When weight > 0 (tiered),
    /// rewards placing strong rowers in boat 0 (the top boat). When
    /// weight < 0 (even speed), rewards placing strong rowers in
    /// boats 1+ (non-top boats) to spread talent. This formulation
    /// never penalizes placement vs benching — it only affects which
    /// boat a rower goes in.
    /// S16 — talent ordering across boats. Positive weight = tiered
    /// (concentrate talent at the top). Negative = even speed (spread
    /// talent to lower boats). Reward decays by boat rank:
    ///
    /// | Rank | Factor (positive w) | Factor (negative w) |
    /// |------|---------------------|---------------------|
    /// | 0    | 1.0                 | 0 (skip)            |
    /// | 1    | 0.75                | 0.75                |
    /// | 2    | 0.50                | 0.50                |
    /// | 3    | 0.25                | 0.25                |
    /// | 4    | 0.125               | 0.125               |
    /// | 5+   | ~0                  | ~0                  |
    ///
    /// For tiered: boat 0 gets full quality reward, boat 1 gets 75%,
    /// etc. For even speed: boats 1+ get reward for strong rowers
    /// (spreading talent down), decaying so the solver still puts
    /// *some* preference on boat 1 > boat 2.
    pub(crate) fn post_s16_top_boat_stacking(&mut self) {
        if self.cfg.top_boat_stacking_weight == 0 || self.boats.len() < 2 {
            return;
        }
        let mut count = 0usize;
        let w = self.cfg.top_boat_stacking_weight;
        let aw = w.unsigned_abs() as i64;

        // Decay factors as thousandths. Each successive boat gets 60%
        // of the previous one's reward, creating a meaningful gradient
        // without making lower boats worthless:
        //   boat 0 = 1000, boat 1 = 600, boat 2 = 360, boat 3 = 216, ...
        let factors: Vec<i64> = (0..self.boats.len())
            .map(|rank| {
                let mut f = 1000i64;
                for _ in 0..rank {
                    f = f * 3 / 5; // 60% of previous
                }
                f
            })
            .collect();

        if w > 0 {
            // Tiered: reward placing strong rowers in each boat,
            // scaled by rank decay.
            for (b_idx, boat) in self.boats.iter().enumerate() {
                let factor = factors[b_idx];
                if factor <= 0 {
                    continue;
                }
                for seat in 1..=boat.seat_count.as_int() {
                    for (r_idx, rower) in self.available.iter().enumerate() {
                        if let Some(&var) = self.x.get(&(r_idx, b_idx, seat)) {
                            // Multiplicative quality: skill × strength / 2.
                            // Uses raw ordinals (1-based) so only the very
                            // bottom (Nov×Wk = 1×1/2 = 0) is invisible.
                            // Int/Wk = 1, Int/Int = 2, Expert/V.Str = 8.
                            let quality = (rower.skill.ordinal() as i64
                                * rower.strength.ordinal() as i64)
                                / 2;
                            let coef = (-(aw * quality * factor) / 1000) as i32;
                            if coef != 0 {
                                self.obj_terms.push(var.scaled(coef));
                                count += 1;
                            }
                        }
                    }
                }
            }
        } else {
            // Even speed: reward placing strong rowers in boats 1+
            // (spreading talent away from the top boat), decaying so
            // the solver still prefers boat 1 over boat 2.
            for b_idx in 1..self.boats.len() {
                let boat = self.boats[b_idx];
                let factor = factors[b_idx];
                if factor <= 0 {
                    continue;
                }
                for seat in 1..=boat.seat_count.as_int() {
                    for (r_idx, rower) in self.available.iter().enumerate() {
                        if let Some(&var) = self.x.get(&(r_idx, b_idx, seat)) {
                            let quality = (rower.skill.ordinal() as i64
                                * rower.strength.ordinal() as i64)
                                / 2;
                            let coef = (-(aw * quality * factor) / 1000) as i32;
                            if coef != 0 {
                                self.obj_terms.push(var.scaled(coef));
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
        tracing::debug!(terms = count, "S16 top-boat stacking");
    }

    /// S19 — boat-size stacking. Quality reward inversely scaled by
    /// boat size. Strong rowers in smaller boats get a bigger reward
    /// than in larger boats. This encourages the solver to
    /// concentrate talent in 4s/pairs (which need it more) when
    /// trying to equalise boat speeds.
    ///
    /// The scaling factor is `8 / seat_count` (rounded), so:
    /// - pair (2 seats): quality × 4
    /// - four (4 seats): quality × 2
    /// - eight (8 seats): quality × 1
    ///
    /// Only applied when weight > 0.
    pub(crate) fn post_s19_boat_size_stacking(&mut self) {
        if self.cfg.boat_size_stacking_weight == 0 {
            return;
        }
        let mut count = 0usize;
        let w = self.cfg.boat_size_stacking_weight;

        for (b_idx, boat) in self.boats.iter().enumerate() {
            let size_factor = (8i32)
                .checked_div(boat.seat_count.as_int())
                .unwrap_or(1)
                .max(1);
            for seat in 1..=boat.seat_count.as_int() {
                for (r_idx, rower) in self.available.iter().enumerate() {
                    if let Some(&var) = self.x.get(&(r_idx, b_idx, seat)) {
                        let quality = rower.skill.ordinal() + rower.strength.ordinal();
                        let coef = -w * quality * size_factor;
                        if coef != 0 {
                            self.obj_terms.push(var.scaled(coef));
                            count += 1;
                        }
                    }
                }
            }
        }
        tracing::debug!(terms = count, "S19 boat-size stacking");
    }
}
