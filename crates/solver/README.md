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

### H5. Weight-class hard wall per boat
The boat's weight class describes the rowers it's rigged for. A
badly-matched crew makes the boat sit wrong in the water — too light and
the hull rides high and becomes unstable; too heavy and it sits low and
drags. We enforce a generous hard wall around the sum of rower
weight-class ordinals (`Light=1, Medium=2, Heavy=3`):

```
target * N - N  ≤  Σ ordinal(rower) · x[r, b, s]  ≤  target * N + N

where N      = boat.seat_count (cox excluded)
      target = boat_target_weight_ordinal(boat.weight_class)
```

- Cox is excluded from the sum — weight class is about the rowers.
- `Tubby` boats clamp to target `3` (= Heavy). We don't have a Tubby rower
  bucket, so Tubby acts as "at least as heavy as Heavy".
- The wall is `±N` — one full class of average drift. Loose enough that
  feasibility isn't brittle, tight enough to reject e.g. a Heavy-rigged
  eight full of Lights.
- The wall is encoded as two `less_than_or_equals` calls, one with
  positive coefficients (upper bound) and one with negated coefficients
  (lower bound).

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
| S2   | planned      | pair affinities / anti-affinities |
| S3   | planned      | seat affinities |
| S4   | planned      | soft side preference (side_strength > 0) |
| S5   | **enabled**  | weight-class soft target via slack variables |
| S6   | planned      | cox cooldown |
| S7   | planned      | novelty vs recent lineups |
| S8   | planned      | maximise rowers placed |

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

### S2. Pair affinities / anti-affinities
From the `pair_affinity` table. For each stored pair with weight `w`, add
`w · [both rowers on the same boat]` to the objective. Positive `w`
rewards keeping them together; negative `w` penalises it. Reified via a
per-pair-per-boat "both present" boolean.

### S3. Seat affinities
From the `rower_seat_affinity` table. For each `(rower, seat, weight)`,
add `weight · x[rower, b, seat]` across all boats — rewards matching a
rower's preferred seat.

### S4. Side-preference strength
Rowers with `side_strength = 0` remain hard (they can't row the other
side at all). Rowers with `side_strength > 0` become candidates for either
side of the boat, but sitting on their non-preferred side incurs a penalty
proportional to `side_strength`. This replaces the current blanket-hard
side rule.

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

### S6. Cox cooldown
`last_coxed_dates` gives us the most recent cox appearance per rower,
derived from lineup history. For non-designated coxes, coxing within
N days of their last cox appearance incurs a large penalty. Designated
coxes are exempt.

### S7. Novelty vs recent lineups
Minimise overlap with the last K practices. For each `(rower, boat,
seat)` triple that has appeared in the recent window, add a small
penalty when the solver places the same rower in the same seat of the
same boat. Prevents "every Tuesday is the same crew" drift.

### S8. Maximise rowers placed
Soft bias toward fielding more boats / leaving fewer rowers on the dock.
Counterbalances weight-class and skill penalties when they'd otherwise
prefer a smaller-but-tighter crew.

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

The pipeline is in place; adding a new soft term is now a purely additive
change:

1. Create whatever auxiliary decision / slack variables the term needs.
2. Post linear constraints linking them to the existing `x[r,b,s]`
   variables (e.g. "var = 1 iff both rowers of a pair are in the same
   boat" via reified equality).
3. Append the new variables to `slack_vars` (or to a separate weighted
   term list if we start weighting). They'll automatically flow into the
   objective via the `objective = Σ slack_vars` equality.
4. No changes to the solve call or result handling.

Weights are still implicit (all 1). When we introduce per-term weighting,
replace the flat `slack_vars: Vec<DomainId>` with a pair of `Vec`s —
variables and their coefficients — and scale each term with
`var.scaled(weight)` before summing into the objective.

**Top-N alternatives** via tabu re-solve also lands naturally now:
optimise once, record the solution, add a linear constraint that forces
the placement of at least K rowers to differ from the recorded solution,
re-solve. The `NoCallback` stub becomes a real callback that records
every improving solution during search.

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
