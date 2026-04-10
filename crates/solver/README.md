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
  `Skill`, `Strength`, `Height`) rather than numeric measurements. This is a
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
variable. The objective is a sum of per-constraint slack / penalty
terms, each of which is linked to the objective via a linear equality.
Per-constraint weights live in `SolverConfig` on the `SolveRequest`
(see below) — defaults preserve the historical behaviour (mostly 1,
cox cooldown = 5).

| Term | Status       | Config field            | Notes |
|------|--------------|-------------------------|-------|
| S1   | **enabled**  | `skill_variance_weight` | skill variance within a boat (max − min) |
| S2   | **enabled**  | `pair_affinity_weight`  | pair affinities (2-seat partition reification) |
| S3   | **enabled**  | `seat_affinity_weight`  | seat affinities (stored in `rower_seat_affinity`) |
| S4   | **enabled**  | `side_preference_weight`| soft side preference, scaled by `side_strength` |
| S5   | **enabled**  | `weight_class_slack_weight` | weight-class soft target via slack variables |
| S6   | **enabled**  | `cox_cooldown_penalty`  | cox cooldown (non-designated, derived from history) |
| S7   | **enabled**  | `novelty_weight`        | novelty vs recent lineups (per-lineup Hamming, opt-in via `novelty_factor`) |
| S8   | **enabled**  | `placement_reward_weight` | maximise rowers placed (per-boat seat reward + `use[b]`) |
| S9   | **enabled**  | `pair_strength_weight`  | pair strength balance (universal, not data-driven) |

### SolverConfig — per-constraint tunable weights

`SolverConfig` is a plain struct on `SolveRequest` whose fields scale
each soft constraint's contribution to the objective. Defaults:

| Field | Default | What it scales |
|---|---|---|
| `skill_variance_weight` | 1 | S1 per-boat `max − min` spread |
| `pair_affinity_weight` | 1 | S2 per-pair scaled coefficient (multiplies the stored `pair_affinity.weight`) |
| `seat_affinity_weight` | 1 | S3 per-entry scaled coefficient (multiplies the stored `rower_seat_affinity.weight`) |
| `side_preference_weight` | 1 | S4 wrong-side penalty (multiplies the rower's `side_strength`) |
| `weight_class_slack_weight` | 1 | S5 weight-class slack `over` and `under` coefficients |
| `cox_cooldown_penalty` | 5 | S6 flat per-rower penalty when coxing inside the cooldown window |
| `novelty_weight` | 1 | S7 per-historical-lineup similarity penalty |
| `placement_reward_weight` | 1 | S8 per-boat `-seats_total` placement reward |
| `pair_strength_weight` | 1 | S9 per-partition `max − min` strength diff |
| `bow_pair_strength_weight` | 2 | S9b extra weight on the (1, 2) bow-pair strength diff, layered on top of S9 |
| `height_balance_weight` | 1 | S10 per-partition `max − min` height diff |
| `end_pair_skill_weight` | 1 | S11 8-boat end-pair skill reward (seats 1/2/7/8) |
| `engine_room_strength_weight` | 1 | S12 8-boat engine-room strength reward (seats 3-6) |
| `partial_fill_bonus` | 1 | Per-filled-optional-seat reward under `Allowed(k)` partial-fill; inert under `Strict` |
| `non_scull_retention_weight` | 2 | S13 per-sweep-only-rower placement reward; biases "who gets benched" toward can_scull rowers |

**Zero disables the constraint entirely.** Setting any field to `0`
skips the entire constraint block at solve time — no auxiliary
variables created, no linking constraints posted, no `obj_terms`
pushed. This is both the performance-friendly path (Pumpkin panics
on `.scaled(0)` anyway) and the semantic "turn it off" knob. Useful
for A/B experiments or disabling a noisy signal on a particular
solve without editing the code.

**Per-entity weights multiply.** Constraints that already carry a
per-entity weight (S2 stored pair weights, S3 stored seat weights,
S4 per-rower `side_strength`, S8 per-boat `seats_total`) combine that
weight with the config multiplier via simple multiplication. So
`pair_affinity_weight = 2` doubles the effect of every stored
`pair_affinity.weight` rather than replacing it. The stored values
still carry the coach's judgement about which rowers are more
important; the config is a global dial.

**Negative values invert the constraint.** A negative
`skill_variance_weight` would reward high skill spread, a negative
`placement_reward_weight` would penalise fielding boats, and so on.
This is a footgun but occasionally useful for experiments. The type
system doesn't prevent it; stick to non-negative values for normal
coach use.

**CLI/config-file loading — future work.** Right now the CLI uses
`SolverConfig::default()` unconditionally. A future enhancement would
load a TOML config file (or per-club profile) via a `--config PATH`
flag. Deferred until there's a concrete admin UI surface calling for
it.

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
pays a penalty if it tries to seat them as cox again. The penalty
**decays linearly** with `days_since_last_cox` so "cold" rowers are
strictly preferred over "hot" ones even when both are inside the
window. Designated coxswains are exempt — they're meant to cox often.

**Constants** (in `crates/solver/src/lib.rs`):
- `COX_COOLDOWN_DAYS = 14` — roughly two weeks of practice, the
  rotation horizon most clubs care about ("don't have the same person
  cox two weeks running"). Long enough to matter, short enough not to
  effectively lock out anyone who ever coxes.
- `SolverConfig::cox_cooldown_penalty = 5` — the *maximum* per-rower
  penalty (applied when `days_since = 0`). Same ballpark as S4
  wrong-side penalty for a strongly side-locked rower.

**Linear decay formula.** The effective penalty for a rower who
coxed `days_since` days ago is

```
effective = ceil(cox_cooldown_penalty × (COX_COOLDOWN_DAYS − days_since) / COX_COOLDOWN_DAYS)
```

Ceiling division keeps every day inside the window at effective ≥ 1
(otherwise days 12–13 at the default penalty of 5 would round down
to 0 and contribute no pressure). Example with defaults:

| `days_since` | `effective` |
|---|---|
| 0  | 5 |
| 1  | 5 (ceil of 4.64) |
| 3  | 4 |
| 7  | 3 (ceil of 2.5) |
| 10 | 2 |
| 13 | 1 |
| 14+ | 0 (outside the window, block skipped) |

The curve is smooth end-to-end: a rower 13 days out still costs
something, but *much* less than a rower 1 day out, and the solver
picks the coldest candidate when given the choice.

**Encoding** mirrors the S4 per-rower aggregation:

```
for each rower r where
  !is_designated_cox(r)
  AND last_coxed[r] exists
  AND (request.date - last_coxed[r]) ∈ [0, COX_COOLDOWN_DAYS):

    effective[r] = ceil(cox_cooldown_penalty * (COX_COOLDOWN_DAYS - days_since) / COX_COOLDOWN_DAYS)
    cox_vars[r]  = [x[r, b, 0] for each boat b]   (cox-seat x vars only)
    cox_use[r]   ∈ {0, 1}                          (by H2)
    Σ cox_vars[r] - cox_use[r] = 0
    obj_terms.push(cox_use[r].scaled(effective[r]))
```

One aux var + one linking equality + one obj term per penalized
rower, same as before — the linear decay only changes the scalar
coefficient, not the constraint shape. O(rowers in cooldown) cost.

**Naturally inert cases**:
- Designated coxes: skipped outright (exempt).
- Rowers who've never coxed: no history entry, no penalty.
- Coxes outside the cooldown window: skipped.
- Coxless candidate fleet: no cox-seat x vars exist, `cox_vars` is empty, skipped.

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

### S9b. Bow-pair strength balance (extra) — **enabled**
The (1, 2) bow pair has outsized influence on set and steering. A
strength mismatch there translates into visible set problems and
course corrections that compound for the rest of the crew, so bow
pair matching matters more than any other partition.

Encoding: an *additional* `diff.scaled(bow_pair_strength_weight)`
term is pushed into `obj_terms` on top of the regular S9 per-partition
term, **but only for the (1, 2) partition** of each boat. Under the
defaults (`pair_strength_weight = 1`, `bow_pair_strength_weight = 2`)
the bow pair's effective strength-diff cost becomes `1 + 2 = 3×` a
regular partition's cost.

This is intentionally a *layered* weight rather than a replacement —
zeroing `bow_pair_strength_weight` leaves the regular S9 penalty
intact on the bow pair, while zeroing `pair_strength_weight` still
leaves S9b's bow-specific term firing. That gives the two knobs
independent semantics (universal pair balance vs. bow-specific
emphasis).

Applies to every boat with a partition at seats (1, 2), not just
eights — the bow pair's structural role is the same on a four.

### S10. Pair height balance — **enabled**
Within a 2-seat partition, the two rowers should be roughly the same
height. Pairing a Short rower with a VeryTall one works fine
mechanically but the rigging / catch timing / oar-handle height feel
better when heights match. This is a **gentle** preference — rowable
anyway, but worth nudging — hence the modest default weight.

Encoding mirrors S9 with a `Height` ordinal instead of `Strength`:

- Shared `seat_height_by_seat[(b, s)] ∈ [0, 4]` built once in the
  prelude via `build_seat_trait_map`. Linked via
  `Σ_r ordinal(rower.height) · x[r, b, s] - seat_height[b, s] = 0`.
- Per partition `(s_lo, s_hi)`: `pair_max` / `pair_min` / `diff` via
  the same `maximum` / `minimum` / linear-equality pattern as S9.
- `diff.scaled(height_balance_weight)` pushes into `obj_terms`.

`Height::ordinal()` starts at **1** (Short=1 .. VeryTall=4), same
zero-coefficient-avoidance reason as Skill/Strength.

**Interaction with partial-fill.** Optional seats are included in
the shared `seat_height_by_seat` map under `Strict` partial-fill
(where they're guaranteed filled) and excluded under `Allowed`
(where an empty seat would drive the trait var to 0 and poison the
pair diff). Under `Allowed`, the (3,4) partition of a partial-fill
8+ falls through to the `None` arm of the lookup and is silently
skipped — same behaviour as S9.

### S11. End-pair skill reward — **enabled** *(8-boats only)*
In an eight, seats 1/2 are the bow pair and seats 7/8 are the stern
pair. Both are high-skill positions but for different reasons: the
stern pair leads the stroke and sets rhythm for the rest of the
crew; the bow pair is where set and steering live — balance
problems and course corrections both originate there. Both jobs
reward technical skill over raw power, so we nudge the solver to
put the most technically skilled rowers in the ends of an eight.

The bow pair's separate sensitivity to *strength mismatch* (a
distinct concern from skill) is handled by the S9b
`bow_pair_strength_weight` term, not here.

**Encoding.** A pure objective-side term with no new auxiliary
variables or linking constraints:

```
for each 8-boat b:
  for seat s ∈ {1, 2, 7, 8}:
    obj_terms.push(seat_skill_by_seat[(b, s)].scaled(-end_pair_skill_weight))
```

The objective is minimised, so a negative-coefficient term
effectively *maximises* the wrapped variable's value. For an unused
boat, every `x` is 0 → `seat_skill = 0` → the term contributes
nothing, so there's no phantom "reward for benching a boat" effect.

**Piggybacks on the shared skill map.** The prelude builds
`seat_skill_by_seat` whenever `skill_variance_weight ≠ 0` **or**
`end_pair_skill_weight ≠ 0`, so S11 reuses S1's per-seat aux vars
rather than creating a parallel set.

**8-boats only.** A 4-boat has no engine-room/ends distinction — the
whole thing is ends. Smaller boats are even more so. The constraint
is gated on `boat.seat_count == 8` inside the loop; 4-boats and pairs
are skipped silently.

### S12. Engine-room strength reward — **enabled** *(8-boats only)*
Seats 3, 4, 5, 6 of an eight are the "engine room" — the four middle
seats that provide the bulk of the propulsive power. Unlike the
end pairs, their job is pure force application, so we reward placing
the strongest rowers here.

**Encoding** is structurally identical to S11 but over the
`seat_strength_by_seat` map and the engine-room seat set:

```
for each 8-boat b:
  for seat s ∈ {3, 4, 5, 6}:
    obj_terms.push(seat_strength_by_seat[(b, s)].scaled(-engine_room_strength_weight))
```

Same piggyback pattern: `seat_strength_by_seat` is built when either
`pair_strength_weight ≠ 0` or `engine_room_strength_weight ≠ 0`, so
S9 and S12 share the per-seat aux vars.

**Interaction with S11.** They're complementary — S11 pulls skilled
rowers toward the ends, S12 pulls strong rowers toward the middle.
For a rower who is both (e.g. an Expert/VeryStrong), the solver
weighs the two rewards against each other and places them where the
marginal gain is larger. Most real rowers aren't maxed on both axes,
so in practice the two constraints tend to partition the crew
naturally along the skill/strength diagonal.

### S13. Non-scull retention — **enabled**
Without any per-rower preference, S8's per-boat placement reward
treats every available rower the same: two solutions that differ
only in *who* sits on the dock are tied to the objective and the
solver picks one arbitrarily. That's wrong from the coach's
perspective — a rower flagged `can_scull = true` has somewhere to
go (the scullers team), whereas a sweep-only rower benched today
just sits out.

S13 fixes the tie-break. For every available rower with
`can_scull = false`, push a per-rower retention reward into the
objective:

```
for each available rower r where !r.can_scull:
    rower_used[r] ∈ {0, 1}              (aux var; H2 makes the bound tight)
    Σ_{b, s} x[r, b, s] − rower_used[r] = 0
    obj_terms.push(rower_used[r].scaled(-non_scull_retention_weight))
```

When the solver places rower r anywhere, `rower_used[r] = 1` and
the objective drops by `non_scull_retention_weight`. When r is
benched, `rower_used[r] = 0` and no reward accrues. Sculling-
eligible rowers get no extra term — they cover their own fallback
via the existing S8 placement signal, and adding the retention
bonus uniformly would just shift a constant without breaking any
tie.

**Encoding cost.** Same per-rower aggregation pattern as S4 / S6
— one aux var, one link equality, one obj term per non-scull
rower. O(non-scull rowers) total contribution to `obj_terms`. No
seat-level explosion.

**Default weight.** `2`, same ballpark as a single S1 skill-
spread unit and slightly lower than the cox cooldown penalty.
Big enough to break "who do I bench" ties decisively, small
enough that it can't outvote S1 / S5 / S9 / S11 / S12 structural
preferences when those would prefer a non-scull rower stays
benched (e.g. their skill or strength would actively hurt the
boat).

**Interaction with the unplaced output.** The `SolveResult`
returned by `solve()` carries an `UnplacedRowers` breakdown
split into `to_sculling` (can_scull = true) and `benched`
(can_scull = false) — coaches see exactly which fallback the
solver implicitly assumed. S13 makes the `benched` list as
short as the constraints allow, with `to_sculling` absorbing
the overflow.

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

**Top-N alternatives — enabled.** `SolveRequest.top_n` (default 1)
asks for N distinct lineups instead of just the best. `solve()`
runs one full `optimise()` call to produce the primary solution,
collects the set of `x = 1` placements, and posts a linear tabu
constraint before the next call:

```
Σ_{v ∈ previous placements} v  ≤  |previous| − tabu_min_diff
```

Every successive iteration adds another tabu constraint against
its own output, so the N-th alternative differs from all N−1
previous ones by at least `SolveRequest.tabu_min_diff` placements
(default 2 — one rower swap between two seats). The loop stops
early if the accumulated tabus leave an empty feasible region
(Pumpkin returns `Unsatisfiable`) or if `tabu_min_diff` exceeds
the placement count, in which case the result carries fewer
alternatives than requested rather than erroring.

**Budget caveat.** `time_budget` is per-alternative, not total —
top_n = 3 with budget = 10s can take up to 30 seconds wall-clock
because each Pumpkin invocation burns its own clock. For
interactive use shrink the per-call budget (e.g. 2s × 5
alternatives = 10s total).

**Ranking.** Alternatives are returned best-first. Because each
tabu constraint makes the previous optimum infeasible, every
subsequent solve's best objective is ≥ the previous one, so
`SolveResult.alternatives[0]` is the 2nd-best lineup,
`alternatives[1]` is the 3rd-best, etc.

**Only the primary is persisted.** `cargo run -p lineup_cli --
solve --alternatives 3 --commit` commits only the primary; the
alternatives are shown for reference but the DB layer has no
notion of "proposed alternatives" yet. A future enhancement
would store all N as a bundle the coach can pick from via a web
UI.

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

**Interaction with the shared seat-trait maps.** The shared
`seat_{skill,strength,height}_by_seat` maps that feed S1 / S9 /
S9b / S10 / S11 / S12 are built with a **policy-aware skip rule**:
- Under `Strict` partial-fill, every seat is required so optional
  seats are *included* — S9 pair strength diff, S10 pair height
  diff, and S12 engine-room strength all correctly see partition
  (3, 4) of an 8+.
- Under `Allowed(k)`, optional seats are *excluded* from the maps
  because an empty seat would drive the trait var to 0 and pollute
  any max / min / spread / diff calculation downstream. This means
  S9's partition (3,4) is silently skipped, S10's too, and S12
  loses the seat-3 and seat-4 engine-room rewards.

S11 end-pair skill is unaffected either way — end-pair seats
`{1, 2, 7, 8}` never overlap the optional set.

Prior to the policy-aware rule, optional seats were *always*
excluded from the shared maps, which meant under `Strict` the
pair-(3,4) balance constraints were silently dormant on every 8+.
The unit tests in `crates/solver/tests/constraints.rs` caught the
bug via an isolated S12 test.

**CLI.** `cargo run -p lineup_cli -- solve --partial N [date]`
switches the policy to `Allowed(N)`. `--partial 0` is equivalent to
`Strict`.

**When this bites.** The `Allowed(k)` policy lets the solver
field an 8+ with `k` of its optional seats empty. With the
partial-fill bonus on (the default), the solver still strictly
prefers full fills when there are enough rowers to fill the
boat, so the partial-fill path only actually produces an empty
seat when either (a) there genuinely aren't enough rowers to
fill it, or (b) the structural soft cost of squeezing a
specific rower into seat 3 or 4 outweighs the full-fill bonus
plus their S8 contribution. With the toy fixture (11 available,
3 candidate boats), `Allowed(2)` still produces a full-fill
Persephone because there are enough rowers for it.

**Partial-fill bonus — enabled.** Without any objective-side
pressure, the solver is indifferent between "field Persephone
with seats 3 and 4 filled" and "field Persephone with seats 3
and 4 empty" under `Allowed(2)` — the S8 per-boat reward
(`seats_total · use[b]`) doesn't know or care how many of those
seats are actually occupied, so both outcomes produce the same
objective contribution even though one benches rowers
needlessly. Tie-broken by search order → silently unpredictable.

Fix: `SolverConfig::partial_fill_bonus` (default 1) pushes a
per-optional-seat reward of

```
Σ_r x[r, b, opt_seat] .scaled(-partial_fill_bonus)
```

so a filled optional seat contributes `-partial_fill_bonus` to
the objective and an empty one contributes 0. The solver,
minimising, strictly prefers the filled case by exactly
`partial_fill_bonus` units per optional seat. Under `Strict`
partial-fill the bonus is gated off entirely because H1's
equality already forces every seat filled — no reason to pay
for extra obj terms that contribute a constant.

**Scale.** Default `1` breaks the "leave it empty" tie without
overriding structural soft terms like S1 skill variance, S9
pair strength, S11, or S12. If a specific rower can only fit in
seat 3 at a cost that exceeds the combined structural penalty,
the solver will still leave the seat empty — the bonus isn't a
hard "must fill" rule, it's a gentle nudge. Raise the value to
make full fills harder to beat; lower (or zero) it to recover
the pre-bonus behaviour.

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
