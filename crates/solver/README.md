# lineup_solver

Constraint-solver core for the GGRC sweep-lineup generator. Takes a
`DbSnapshot` plus a `SolveRequest` (which boats to field) and returns one or
more proposed lineups (one per requested boat), or reports infeasibility.

This document is the durable reference for *what* the solver is trying to do
and *why* — the code in `src/lib.rs` is the source of truth for how. When a
constraint feels wrong in practice, come back here first to check whether the
intent has drifted from the implementation.

## Scope

- **Sweep only.** Sculling is a separate team at GGRC. A rower whose
  availability status is `ScullingOnly` is tracked in the system (the shared
  spreadsheet syncs them the same way) but is excluded from sweep seat
  candidacy. `can_scull` on a rower is an overflow-eligibility flag — it
  lets a later milestone recommend "push this rower to the scullers today"
  as an alternative to benching them, but it does **not** drive sculling
  lineup generation.
- **Standard alternating rig only.** Seats alternate strictly from the
  stroke seat based on `boat.stroke_side`. Bucket / German / Italian rigs
  (two adjacent seats on the same side) are not modelled. The escape hatch
  is a future per-seat override table.
- **Bucketed data.** Rower traits are coarse enums (`RowerWeightClass`,
  `Skill`, `Strength`) rather than numeric measurements. This is a
  deliberate choice to minimise admin data-entry friction; the solver only
  needs enough signal to distinguish categories.
- **Coach-supplied fleet for now.** Boat selection is currently driven by
  the `greedy_fleet_selection` helper in the CLI or passed explicitly in
  `SolveRequest.boats`. Pushing boat selection *inside* the model is a
  planned milestone; see "Future work" below.

## Variable encoding

The model is a pure 0/1 assignment matrix:

```
x[rower_idx][boat_idx][seat_position] ∈ {0, 1}
```

A variable is created only for combinations where the rower is **eligible**
for the seat. Ineligible triples simply don't exist in the model, which is
cleaner and slightly faster than creating a dead variable with a fixed
domain of `{0}`. Eligibility is a conjunction of:

- The rower's availability status for this date is `Yes`.
- If the seat is cox (position 0), the rower has `can_cox = true`.
- If the seat is a rowing seat, the rower's `side` matches
  `boat.seat_side(seat)` — or the rower is `Either`.

Seat positions are `0` for cox (only on coxed boats) and `1..=seat_count`
for rowing seats, numbered bow → stroke.

## Hard constraints

These are enforced absolutely. If any of them can't be satisfied, the
solver returns `SolveStatus::Unsatisfiable`.

### H1. Each requested seat is filled by exactly one rower
```
for each (boat b, seat s in b):    Σ_r x[r, b, s] = 1
```
This is what drives the search away from the trivial all-zero solution;
there is no "don't field this boat" escape hatch inside the current model —
boat selection happens outside the solver.

### H2. Each rower occupies at most one seat overall
```
for each rower r:    Σ_{b, s} x[r, b, s] ≤ 1
```
Surplus rowers (available but no seat fits them) sit out.

### H3. Cox / designated-cox eligibility
Two related rules, both enforced via eligibility filtering (no explicit
posted constraint):

- Seat 0 (cox) only accepts rowers with `can_cox = true`.
- A rower with `is_designated_cox = true` is rejected from *all* rowing
  seats (1..=N). They only ever cox, never row. This is an absolute
  rule regardless of availability, side, or weight class.

The data-level invariant is that `is_designated_cox` implies `can_cox`;
if the former is true and the latter false, the rower is effectively
unplaceable, which is a data hygiene bug worth surfacing in the coach
UI later.

### H4. Side matching
A rower is only a candidate for a seat whose side matches their `side`
attribute. `Either` rowers match both sides. Hard for all rowers regardless
of `side_strength` at the moment — the per-rower soft-preference path moves
to the objective function in a later milestone (see "Planned soft
constraints"). Also implemented via eligibility filtering.

### H5. Weight-class hard wall per boat (upper bound only)
The boat's weight class describes the rowers it's rigged for. A
badly-matched crew makes the boat sit wrong in the water — too light and
the hull rides high and becomes unstable; too heavy and it sits low and
drags. We enforce an UNCONDITIONAL upper bound on the sum of rower
weight-class ordinals (`Light=1, Medium=2, Heavy=3`):

```
Σ ordinal(rower) · x[r, b, s]  ≤  target * N + N

where N      = boat.seat_count (cox excluded)
      target = boat_target_weight_ordinal(boat.weight_class)
```

- Cox is excluded from the sum — weight class is about the rowers.
- `Tubby` boats clamp to target `3` (= Heavy). We don't have a Tubby
  rower bucket, so Tubby acts as "at least as heavy as Heavy".
- The wall is `+N` — one full class of average drift over-target.
- The upper bound is unconditional (no dependence on `use[b]`): when
  the boat is unused, sum = 0 which is trivially ≤ anything positive.

**The corresponding *lower* bound is intentionally absent.** A
conditional lower bound (active only when `use[b] = 1`) would need a
big-M formulation, and big-M dramatically weakens CP propagation —
during development it blew solve time from milliseconds to 78 seconds.
Preventing too-light crews is instead delegated to the S5 soft slack
penalty, which applies strictly positive cost per unit of underweight
and is almost always enough to keep the solver from fielding an
all-Light crew in a Heavy boat (the slack cost per unit exceeds any
offsetting S8 placement reward).

The preference for hitting the exact target lives in the objective
function as a slack-variable penalty — see S5 below.

## Soft constraints (objective function)

The solver calls `solver.optimise()` with `LinearSatUnsat` +
`OptimisationDirection::Minimise` against a single integer objective
variable. The objective is a sum of per-constraint slack / penalty terms,
each of which is linked to the objective via a linear equality. Weights
are intended to live in a runtime config file so the coach can tune them
without recompiling — today all enabled terms carry an implicit weight
of 1.

| Term | Status       | Notes |
|------|--------------|-------|
| S1   | **enabled**  | skill variance within a boat (max − min) |
| S2   | **enabled**  | pair affinities (2-seat partition reification) |
| S3   | **enabled**  | seat affinities (stored in `rower_seat_affinity`) |
| S4   | **enabled**  | soft side preference, weighted by `side_strength` |
| S5   | **enabled**  | weight-class soft target via slack variables |
| S6   | **enabled**  | cox cooldown (non-designated, derived from history) |
| S7   | **enabled**  | novelty vs recent lineups (per-lineup Hamming distance, opt-in) |
| S8   | **enabled**  | maximise rowers placed (per-boat seat reward + `use[b]`) |
| S9   | **enabled**  | pair strength balance (universal, not data-driven) |

### S1. Skill variance within a boat — **enabled**
Avoid putting eight novices with one expert in an eight. Encoded as a
per-boat `spread[b] = boat_max[b] - boat_min[b]` where `boat_max` and
`boat_min` come from Pumpkin's `maximum` / `minimum` constraints over
per-seat skill auxiliary variables:
- `seat_skill[b,s] ∈ [1, 4]` created for each rowing seat.
- Linked via `Σ_r ordinal(rower.skill) · x[r,b,s] - seat_skill[b,s] = 0`.
  Because exactly one x is 1 per filled seat (H1), `seat_skill[b,s]`
  takes the placed rower's skill ordinal.
- `boat_max[b]`, `boat_min[b] ∈ [1, 4]` via `maximum` / `minimum`.
- `spread[b] ∈ [0, 3]` via `boat_max - boat_min - spread = 0`.
- `spread[b]` appends to `slack_vars`.

Skill ordinals start at **1** (Novice=1 .. Expert=4), not 0. Pumpkin
panics on `.scaled(0)`, so we shift the ordinals up by one. Spread is
max-minus-min and is invariant under the shift.

### S2. Pair affinities — **enabled**
From the `pair_affinity` table. In rowing, a "pair" is a fixed 2-seat
partition of a boat: `(1,2), (3,4), (5,6), (7,8)` — **not** every
adjacent pair of seats. `(seat 2, seat 3)` is not a pair because it
crosses a partition boundary.

For each stored `(A, B, w)` and each boat, the solver iterates over the
boat's partitions and introduces a reified boolean
`together[pair, boat, partition] ∈ {0,1}` driven by the AND of "A is in
this partition" and "B is in this partition":

```
A_in_partition := x[A, b, s_lo] + x[A, b, s_hi]   (at most 1 by H2)
B_in_partition := x[B, b, s_lo] + x[B, b, s_hi]   (at most 1)

together ≤ A_in_partition
together ≤ B_in_partition
together ≥ A_in_partition + B_in_partition − 1
```

Three linear inequalities per (stored_pair × boat × partition). Then
`together.scaled(-w)` pushes into `obj_terms`. Positive `w` rewards
pair-sharing; negative `w` penalises it.

**Naturally inert cases** (no error, just no effect):
- Either rower unavailable → no `x` vars exist, partition terms are
  empty, the partition is skipped.
- Designated cox involved → cox has no rowing-seat variables, same
  result.
- Structurally incompatible pair (e.g. both Port under hard side-locks)
  → the reified boolean stays 0 for every partition; no reward earned
  but no error.

**Standard-rig assumption.** Under standard alternating rig, every
partition contains exactly one port and one starboard rower, so pair
affinities are implicitly opposite-sided. Double-bucket rigs (two
adjacent seats on the same side) break that invariant: a partition in
a double-bucket boat may contain two same-side rowers, and the
"pair = port + starboard" expectation no longer holds. We currently
don't model bucket rigs at all (see §Scope), so this is a deferred
concern; if we add per-seat side overrides later, pair affinity
semantics under non-standard rigs will need revisiting.

**Future: cox-crew affinity.** Coxswains tend to be associated with
particular boats or crews — a future soft term could reward matching
a cox with a crew they've been working with. Not implemented; tracked
here for visibility.

### S3. Seat affinities — **enabled**
From the `rower_seat_affinity` table. For each `(rower, seat, weight)`
stored, the solver pushes `x[r,b,seat].scaled(-weight)` into `obj_terms`
for every boat where that seat position exists. Positive weights become
negative contributions (rewards); negative weights become positive
contributions (penalties).

`seat_position` is boat-agnostic — a preference for seat 8 only applies
to 8-boats, and a preference for seat 4 applies to 4-boats *and* to
seat 4 of 8-boats. Loaded into `DbSnapshot::seat_affinities` in one
shot from the full table (there aren't enough rows to bother filtering
per-date).

Because positive affinities contribute negative values, the objective
variable's domain is widened to `[-10_000, 10_000]`. Pumpkin happily
propagates tighter bounds from the term domains during search.

### S4. Side-preference strength — **enabled**
Rowers with `side_strength = 0` remain hard-locked — the eligibility
filter rejects wrong-side placements entirely, so no variable is created.
Rowers with `side_strength > 0` become candidates for either side of the
boat, and wrong-side placements add a `side_strength`-scaled term to
`obj_terms` at variable-creation time:

```
obj_terms.push(x[r,b,s].scaled(rower.side_strength))
```

`Either`-sided rowers and on-side placements contribute zero. The
default fixture uses `side_strength = 3` for everyone, so the solver
strongly prefers correct sides but will accept a wrong-side placement if
the alternative (benching a critical rower, failing to field a boat)
costs more.

### S5. Weight-class fit (soft target) — **enabled**
Complements the hard wall in H5 with a soft target that prefers the exact
class fit:
- Nonneg slack variables `over[b]`, `under[b]` per boat, each bounded in
  `[0, 3N]`.
- Linear equality `Σ ordinal · x - over[b] + under[b] = target * N`.
  At optimum only one side is nonzero and together they equal
  `|sum - target * N|`.
- All slacks are summed into the single objective variable with weight 1.
  Future weighting (when more soft constraints join) scales each pair
  independently.

In the current fixture this is the *only* objective term, so the solver
is effectively minimising "total deviation from every boat's target". As
S1–S8 arrive they all feed into the same objective sum.

### S6. Cox cooldown — **enabled**
`snapshot.last_coxed` gives us the most recent cox appearance per rower,
derived from `lineup_seat` history via `Rower::last_coxed_dates`. For
non-designated rowers who coxed within the cooldown window, the solver
pays a flat penalty if it tries to seat them as cox again. Designated
coxswains are exempt — they're meant to cox often.

**Constants** (in `crates/solver/src/lib.rs`):
- `COX_COOLDOWN_DAYS = 14` — roughly two weeks of practice, the
  rotation horizon most clubs care about ("don't have the same person
  cox two weeks running"). Long enough to matter, short enough not to
  effectively lock out anyone who ever coxes.
- `COX_COOLDOWN_PENALTY = 5` — same ballpark as S4 wrong-side penalty
  for a strongly side-locked rower.

**Encoding** mirrors the S4 per-rower aggregation:

```
for each rower r where
  !is_designated_cox(r)
  AND last_coxed[r] exists
  AND (request.date - last_coxed[r]) ∈ [0, COX_COOLDOWN_DAYS):

    cox_vars[r] = [x[r, b, 0] for each boat b]   (cox-seat x vars only)
    cox_use[r] ∈ {0, 1}                          (by H2)
    Σ cox_vars[r] - cox_use[r] = 0
    obj_terms.push(cox_use[r].scaled(COX_COOLDOWN_PENALTY))
```

One aux var + one linking equality + one obj term per penalized rower.
O(rowers in cooldown) cost.

**Naturally inert cases**:
- Designated coxes: skipped outright (exempt).
- Rowers who've never coxed: no history entry, no penalty.
- Coxes outside the cooldown window: skipped.
- Coxless candidate fleet: no cox-seat x vars exist, `cox_vars` is empty, skipped.

**Decay**: currently a flat penalty within the window. A linear decay
like `penalty * (cooldown - days_since) / cooldown` would reward
waiting longer before re-coxing, but the constant form is simpler and
sufficient for the "don't cox two days in a row" signal that matters
most. The constants can migrate into `SolverConfig` when per-team
tuning arrives.

**Demonstration note**: in the toy fixture, Lena is the only
designated cox and the only high-value cox candidate — she wins the
cox competition on other dimensions (S1 skill, her availability, etc.)
regardless of whether S6 penalises Mika. To see S6 visibly reshuffle
the solution you need a fixture where the non-designated cox is the
naturally-preferred choice and the penalty tips the balance. The
constraint is still posted and enforced in the current fixture.

### S7. Novelty vs recent lineups — **enabled**
Penalise *lineups* that are too similar to recently-committed ones.
Similarity is measured per historical lineup (one committed
`(practice, boat)` pair) as the number of matching placements — how
many rowers from history would be sitting in the exact same seat of
the exact same boat if the solver produced today's lineup. Think
Hamming distance between the current lineup and each historical
lineup.

**Opt-in via `SolveRequest.novelty_factor: i32`**:

- `0` (default) — no constraint posted. Exact repeats are fine.
- `1` — deprioritize lineups that are **1 seat (or fewer) different**
  from any historical lineup. An exact 8/8 repeat incurs the biggest
  penalty; "all but 1 seat same" (7/8) incurs a smaller one.
- `2` — extends the penalty band to 2-seat differences. 6/8, 7/8, 8/8
  all penalised, each harder than the last.
- Higher values widen the band and steepen the per-distance penalty.

**Data source** is `snapshot.recent_placements`, the flattened
placements from the last `RECENT_LINEUP_WINDOW` (= 4) committed
practices. The solver groups them by `(practice_date, boat_id)` at
solve time so each historical lineup gets its own constraint.

**Encoding per historical lineup L** (with `N_L` reachable rowing
placements — i.e. placements whose rower is still available today
and whose boat is still in the candidate fleet):

```
threshold = N_L - factor - 1
match_L   = Σ x[r, b, s] for each live (rower, boat, seat) in L
penalty_L ≥ 0                            (by domain)
penalty_L ≥ match_L - threshold          (soft lower bound)
obj_terms.push(penalty_L.scaled(1))
```

The lower bound is posted as the linear inequality
`Σ match_terms − penalty_L ≤ threshold`, which gives
`penalty_L ≥ match_L − threshold`. Pumpkin minimises, so it picks
`max(0, match_L − threshold)` — zero below the threshold, linearly
growing above.

**Numerical check** for `factor = 1` on an `N = 8` historical
lineup:

| match count | penalty |
|---|---|
| 8 (exact repeat) | 2 |
| 7 | 1 |
| 6 | 0 |

For `factor = 2`, `N = 8`:

| match count | penalty |
|---|---|
| 8 | 3 |
| 7 | 2 |
| 6 | 1 |
| 5 | 0 |

**Cox seats are excluded.** Cox rotation is governed by S6 cox
cooldown, which has a designated-cox exemption that S7 would fight
against.

**Naturally inert cases**:
- `novelty_factor = 0`: the constraint is never posted at all.
- Fresh database with no committed history: `recent_placements` is
  empty, the group map is empty, nothing posted.
- Rowers absent today or boats no longer in today's fleet: the
  corresponding match terms don't exist. The historical lineup just
  appears "smaller" than it was, which is correct — placements we
  can't reproduce shouldn't count against novelty.
- Historical lineups where every placement is no longer reachable:
  `reachable_matches == 0`, skipped.
- Thresholds that exceed the reachable match count: constraint is
  trivially slack, skipped.

**Cost budget**: one aux var + one linear inequality per historical
lineup. With `RECENT_LINEUP_WINDOW = 4` and a realistic 10-boat
fleet, that's at most ~40 historical lineups — tiny compared to the
`obj_terms` performance threshold.

**CLI**: `cargo run -p lineup_cli -- solve --novelty N [date]`.
`--novelty 0` or omitting the flag disables S7.

**Broader novelty signals (future work).** The current encoding is
"same exact seat on same exact boat". Variants like "same boat
regardless of seat" (rotates within-boat) or "same pair partition"
(rotates rowing pairs) are plausible extensions; deferred until the
current form proves too tight or too loose in practice.

### S8. Maximise rowers placed — **enabled**
Soft bias toward fielding more boats / leaving fewer rowers on the
dock. Counterbalances weight-class and skill penalties when they'd
otherwise prefer a smaller-but-tighter crew.

**This is also what enables boat selection inside the model.** Before
S8 the CLI passed a fixed `boats: Vec<BoatId>` and H1 forced every seat
in those boats filled — the solver had no choice over which boats to
field. With S8, the solver owns a per-boat `use[b] ∈ {0,1}` decision
variable that drives a modified H1:

```
for each (boat b, seat s):    Σ_r x[r,b,s] = use[b]
```

If the solver picks the boat (`use[b] = 1`), every seat is filled
exactly once; if not, every seat is empty. Partial-fill is not
supported yet — see "Adding more soft constraints > Partial fill" for
the planned extension.

**Encoding trick: per-boat reward, not per-rower.**
Because H1 is all-or-nothing per boat, rewarding "K rowers placed in
this boat" is equivalent to rewarding `use[b] * K`. The solver pushes
one term per boat into `obj_terms`:

```
obj_terms.push(use[b].scaled(-(boat.seat_count + (1 if cox else 0))))
```

The naive alternative — `obj_terms.push(x[r,b,s].scaled(-1))` for
every x variable — was tried first and made the objective-link
equality a ~100-term monster that Pumpkin took ~78 seconds to solve.
The compact per-boat form drops that to ~1 second and is
mathematically identical given H1.

**Boat selection is driven by objective balance.** A boat is fielded
iff the S8 reward (`seats_total`) exceeds the sum of soft costs
(weight-class slack, skill spread, pair strength diff, side penalty,
etc.) of actually using it. In the current fixture, the 8+ Persephone
has an S8 reward of 9 but paying for the weight-class mismatch (Heavy
boat, mostly Medium crew) costs more than 9, so the solver correctly
skips it in favour of the two 4-boats.

**Coach pinning.** Passing a non-empty `SolveRequest.boats` filters
the candidate fleet — only those boats get `use[b]` variables. This is
a primitive coach override: "only consider these boats". Forcing a
specific boat to be fielded (fixing `use[b] = 1`) is not yet exposed
via the request API but is a one-line addition when needed.

### S9. Pair strength balance — **enabled**
Within a single rowing pair (a 2-seat partition), the two rowers should
have similar strength. Mismatched strength in a pair means one side
pulls harder than the other and the hull yaws off course — matched
strength keeps the boat tracking straight. This is a universal
structural rule, not a coach preference about specific rowers, so it
applies to every partition automatically regardless of the
`pair_affinity` table contents.

Encoding mirrors S1 but scoped to two-seat windows:
- `seat_strength[b, s] ∈ [1, 4]` per rowing seat, linked via
  `Σ_r ordinal(rower.strength) · x[r, b, s] - seat_strength[b, s] = 0`.
  H1 guarantees the sum equals the placed rower's strength ordinal.
- Per partition `(s_lo, s_hi)`: `pair_max` and `pair_min` via
  `maximum` / `minimum` over the two seat_strength vars, then
  `diff = pair_max - pair_min ∈ [0, 3]`.
- `diff.scaled(1)` pushes into `obj_terms`.

`Strength::ordinal()` starts at `1` (Weak=1 .. VeryStrong=4) for the
same reason as `Skill::ordinal()` — Pumpkin panics on `.scaled(0)` and
max/min differences are invariant under the shift.

**Design note: why S9 is separate from S2.** S2 represents coach
preferences about *specific* rower pairings ("Alice and Erin train
together"). S9 is a universal hull-tracking rule that applies to any
pair regardless of who's in it. They share the 2-seat partition concept
but the storage and scope differ: S2 reads from `pair_affinity`; S9 is
computed from strength ordinals on every partition the solver considers.

**Interaction with bucket rigs.** Like S2, S9 assumes standard
alternating rig so that every 2-seat partition is a real opposite-side
pair. Under a double-bucket rig the partition may contain two same-side
rowers and the "matched strength keeps the boat straight" intuition
weakens (same-side rowers don't cancel each other's pull laterally in
the same way). When we add bucket-rig support, S9 would need to look at
which *sides* the two rowers are on, not just their strength.

## Non-goals

Things we deliberately don't model, even if someone asks:

- **Sculling lineup generation.** Scullers are a separate team with
  separate coaches. The sweep solver will never assign rowers to sculling
  boats. (`can_scull` exists as an overflow-eligibility flag only.)
- **Exact rower weights / erg splits.** Bucketed enums (`RowerWeightClass`,
  `Strength`) are deliberate. Asking admins to maintain exact numeric
  weights is a data-entry hole we're not willing to dig.
- **Non-alternating rigs.** Bucket / German / Italian rigs are out of
  scope unless we get a per-seat override table.
- **Boat maintenance state.** That lives in `boat_tracking`. When the two
  apps share a fleet, broken boats will simply not appear in
  `list_in_service` and the solver will never see them.
- **Multi-date scheduling.** One practice at a time. Cox cooldown reads
  prior lineups but we don't plan across future dates.

## Adding more soft constraints

The pipeline is in place; adding a new soft term is now a purely
additive change:

1. Create whatever auxiliary decision / slack variables the term needs.
2. Post linear constraints linking them to the existing `x[r,b,s]`
   (or `use[b]`) variables — e.g. "var = 1 iff both rowers of a pair
   are in the same partition" via reified AND.
3. Append pre-scaled `AffineView<DomainId>` terms to `obj_terms`.
   Positive coefficient = penalty (contributes to minimisation), negative
   coefficient = reward.
4. No changes to the solve call or result handling.

Per-term weights are carried inline via `.scaled(weight)`. When we
introduce per-constraint tunable weights, they just get multiplied into
the existing coefficient at push time.

**Performance note — keep `obj_terms` small.**
Each term appended to `obj_terms` becomes a coefficient in the single
linear equality that links the objective variable to the sum of terms.
Pumpkin's linear propagator scales with that equality's term count, and
*a few hundred terms is already enough to tank solve time from
milliseconds to tens of seconds.* When adding a new soft constraint,
prefer a single per-boat or per-pair term over per-x-variable terms
whenever they're mathematically equivalent (see S8 for a worked
example). If you find yourself pushing N × B × S terms, step back and
see whether an aggregating auxiliary variable would give you the same
objective value with O(B) terms.

**Top-N alternatives** via tabu re-solve land naturally with the
optimise pipeline: optimise once, record the solution, add a linear
constraint that forces the placement of at least K rowers to differ
from the recorded solution, re-solve. The `NoCallback` stub becomes a
real callback that records every improving solution during search.

**Partial-fill strategy — enabled.**
Some clubs prefer fielding an 8+ that's missing seat 3 or 4 (the
inside middle pair) rather than downsizing to a smaller boat and
benching more rowers. Controlled by
`SolveRequest.partial_fill: PartialFillPolicy`:

- `Strict` (default) — every seat of every fielded boat must be
  filled exactly once. Matches the pre-feature behaviour.
- `Allowed(k)` — each fielded boat may have up to `k` of its
  "optional" seats empty. The optional set is hardcoded per boat
  class in `optional_seats(boat)`:
  - 8+: `[3, 4]` (inside bow pair — conventional "row it down a pair")
  - Everything else: `[]` (too structurally unbalanced)

**Encoding.** H1 splits into two cases per seat:

```
Required seat:   Σ_r x[r,b,s] = use[b]     (tight)
Optional seat:   Σ_r x[r,b,s] ≤ use[b]     (may be 0 or 1 when used)
```

Plus a per-boat cap forcing at least `(n_opt − k)` optional seats to
be filled:

```
Σ_{s ∈ opt_seats, r} x[r,b,s]  ≥  (n_opt − k) · use[b]
```

**Interaction with S1 / S9.** Optional seats are excluded from the
S1 skill variance and S9 pair strength aggregations:
- S1 creates `seat_skill` vars only for required seats, so an empty
  optional seat can't pollute the max/min (a boat of 7 Experts + 1
  empty seat would otherwise report spread = 4, nonsense).
- S9 skips any partition that contains an optional seat — for the 8+
  that's partition (3,4), which is exactly the partial-fill target.

**CLI.** `cargo run -p lineup_cli -- solve --partial N [date]`
switches the policy to `Allowed(N)`. `--partial 0` is equivalent to
`Strict`.

**When this bites.** With 10 rowers and the default fixture (2 × 4s
+ 1 × 8+), partial-fill doesn't visibly change the output because the
two full 4s already place 8 rowers (vs a partial 7-of-8 Persephone
placing 7 rowers — strictly worse under S8). The feature bites when
the fleet is constrained such that a partial-filled larger boat is
the only way to place more rowers than a smaller one.

**Future work — partial-fill penalty.** Currently optional seats
carry zero cost when empty beyond the S5 / S8 implicit effects. A
future refinement would add a small per-empty-seat penalty so the
solver prefers full fills when both are feasible, separately tunable
from the other soft weights.

## Why Pumpkin?

Covered in the project-level plan, summarised here:
- Pure Rust — no FFI, no C++ toolchain on the build machine.
- Active development, MiniZinc Challenge credibility via CP-SAT peer group.
- Supports linear (in)equalities, integer domains, optimisation, and the
  global constraints we're likely to want (`element`, `cumulative`,
  `maximum`, `all_different`-equivalent encodings).
- Escape hatch: Pumpkin reads FlatZinc, so if we hit a modelling wall we
  can emit MiniZinc from our `Model` abstraction and swap backends without
  rewriting the constraint logic.

## Future work / open questions

- **Boat selection inside the model.** Replace `greedy_fleet_selection`
  with `use[b] ∈ {0,1}` variables plus fleet constraints, so the solver
  chooses the fleet. Requires the objective function to tie-break between
  equivalent seat totals.
- **Coach pinning.** `practice_boat_constraint` table exists in the
  schema sketch but isn't wired up yet. Will let coaches say "today we
  require the Persephone" or "forbid the Artemis" as part of the request.
- **Per-class weight tolerance.** Currently one formula for all weight
  classes; real clubs may want stricter Light boats and looser Tubby.
- **Rower-specific hard side lock vs soft preference.** Currently all
  side constraints are hard; `side_strength = 0` vs `> 0` distinction
  only meaningful once S4 lands.
- **Warm starts / incremental solving.** If the coach tweaks one
  constraint and re-solves, we should not start search from scratch.
  Pumpkin may or may not support this cleanly; investigate when
  interactive UI lands.
