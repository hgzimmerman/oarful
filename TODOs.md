# lineup_generator — open TODOs

A snapshot of the work backlog as of 2026-04-10. Captures the design
intent behind each pending task so the next session (human or agent)
can resume without re-deriving context. Numbers match the IDs the
task system used while building the project.

## Already shipped

The MVP plus several rounds of follow-ups landed across commits
`0e740b6` through `ea0a6c6`:

- **#45** Sync sheet UI (`POST /sync` form + result panel)
- **#46** Solver knob form on `/solve/{date}` (`SolveKnobs` query
  string + hidden inputs on commit form)
- **#47** Stop double-solving on commit (handled with #46)
- **#49** Inline rower edits on `/rowers` (HTMX outerHTML swap)
- **#50** Per-rower detail page at `/rowers/{id}` with inline pair +
  seat affinity CRUD
- **#52** `Practice::list_committed` query (replaced
  `recent_placements(i64::MAX)` hack)
- **#53** Alternative-vs-primary diff highlighting on solve view
- **#57** Solver latency measurement → dropped `DEFAULT_BUDGET_SECS`
  from 3 → 1 (wall-time was `budget × top_n`)
- **#59** Global semaphore bounding concurrent solver runs
- **#60** Dedicated rayon thread pool for solver work (isolates from
  tokio's blocking pool)
- **#64** Boat CRUD: list / add / edit / relinquish
- **#58** Auth & multi-tenancy (all 5 phases):
  - Phase 1: team structure (team/membership tables, team-scoped
    practice + availability, navbar team selector)
  - Phase 2: master DB + tenant isolation (`crates/master_db/`,
    tenant registry, AppState refactor)
  - Phase 3: JWT auth (user table, login/logout, invite flow, role
    gating, auth middleware)
  - Phase 4: multi-tenant (TenantCache with `Mutex<HashMap>`,
    TenantContext per request, multi-tenant login scan, handler
    refactor to `Extension<TenantContext>`)
  - Phase 5: rower self-service (`/my/profile`, `/my/availability`,
    rower↔user linking, solver role-gated to Coach+)

## Open work

### Foundational (unblocks coach features)

#### #63 — Solver-side seat locks: pre-pin (rower, boat, seat) assignments

Coach use case: "I want Alice in stroke of Persephone, no matter
what — solve the rest around that." Today there's no way to express
this.

**Solver work.** Add a `locks: Vec<SeatLock>` field on `SolveRequest`
where each lock is `(RowerId, BoatId, seat_position)`. In the model
build phase (`lineup_solver/src/model.rs`), for each lock:

1. Find the corresponding `x[r, b, s]` decision variable.
2. Post a hard constraint `x[r, b, s] == 1`.
3. Reject with a structured `Unsatisfiable` reason if the lock
   conflicts with eligibility (wrong side, can't cox, etc.).

Validation: locks must be self-consistent (no two locks on the
same seat, no rower locked into two seats, locked rower must be
available + eligible). Side eligibility and cox checks must happen
before model build with friendly errors.

**Storage.** Recommend per-practice locks (`practice_lock` table:
practice_id, rower_id, boat_id, seat_position). Schema is cheap
and it's the natural model for the no-show workflow (#61).

**UI.** Lock icon per seat on `/solve/{date}`. Click toggles a lock
and re-solves. Locks visually distinct (different bg, lock icon).

**Unblocks:** #61 (no-show via lock-everyone-else).

### Coach features

#### #61 — No-show handling: re-solve with minimal disruption

Coach use case: a rower fails to show up after a lineup is committed.
Mark them as no-show and regenerate with least disruption.

**UI.** On `/history/{date}` add a "no-show" toggle per rower.
Marking no-show overrides availability to `No`, offers a "Re-solve"
button. After re-solve, show a diff against the previous lineup.

**Disruption minimization.** Two approaches:

1. **Lock all unaffected rowers** (depends on #63). For a no-show
   in seat X of boat Y, lock every other (rower, boat, seat) from
   the previous lineup. Cleanest approach.
2. **Reward similarity to baseline** as a new soft constraint (S14).
   More flexible but needs solver work.

Start with approach 1.

**Depends on:** #63 for approach 1; nothing for approach 2.

#### #62 — Manual rower swap in lineups

Coach use case: swap two specific rowers' seats without re-solving.
Pure UI/data feature, not a solver feature.

**Mental model.** The solver proposes; the coach optionally edits;
commit saves whatever's displayed. No re-solving on a swap.

**UI.** Click rower A → highlight → click rower B → swap. Works
across boats. Bench/unplaced lists too (swap a fielded rower with
a benched one). Validation warnings (wrong side, can't cox) but
not blockers — coach overrides.

**State.** Recommendation: **always commit first, edit committed**
(option 3). Coach workflow: solve → commit → optionally edit. The
"commit" verb becomes "save draft". Edits mutate `lineup_seat`
rows directly. No draft table, no hidden form fields.

**No dependency on #63** — locks affect future solves; swap edits
existing lineups. They're orthogonal.

### Quick wins

#### #51 — Practice notes editor

`practice.notes` already exists in the schema. Add a notes textarea
to the solve view (and history view) that POSTs to a new endpoint
and updates `Practice` via `Practice::upsert_by_date(date,
Some(notes))`. Render existing notes on `/history/{date}`.

### Polish

#### #54 — Print-friendly stylesheet for solve / history views

Coaches print the lineup before going on the water. Add `@media
print` rules (or a dedicated `/print/{date}` route) that hides the
navbar, expands all alternatives, drops backgrounds, and lays out
one boat per page-break.

### Productionization

#### #55 — Production static-asset path resolution

`build_router()` takes a `public_dir` string today; `main.rs` reads
`PUBLIC_DIR` env or defaults to `crates/server/public`
(dev-friendly). For deployment, fall back to `exe_dir/public` like
`boat_tracking::build_router` does (multi-path resolution).

#### #56 — Custom Tailwind build pipeline

`templates/layout.rs` currently pulls Tailwind from
`cdn.tailwindcss.com` — fine for dev, broken offline and bloated in
prod. Add a `tailwind.config.js` that scans
`crates/server/src/**/*.rs` for class names, and a build step that
emits `crates/server/public/tailwind.css`.

### Parked

#### #48 — Unsat / timeout diagnostics — solver + UI

Two-part fix:

1. **Solver work first.** Today `SolveResult { status: Unsatisfiable,
   primary: empty, alternatives: empty }` carries no diagnostic
   detail. Extend the solver to surface *why* a model is infeasible
   — at minimum which hard constraint(s) contributed. Approaches:
   - Thread Pumpkin's conflict explanation through `SolveResult`.
   - Or run a relaxation pass: re-solve with each hard constraint
     individually disabled, report which removal unblocks.
   - For `Timeout`: surface best-found-so-far.
2. **UI component.** Replace the one-liner `status_banner` with a
   richer error that explains the failure and offers next steps.

Parked because the solver-side work needs a deeper Pumpkin API dive.

## Follow-ups not yet tracked as tasks

These came up during the auth/multi-tenancy work and aren't yet
formal tasks:

- **CLI `create-tenant` command** — superuser creates a new tenant
  via `cargo run -p lineup_cli -- create-tenant --name "Club" --slug
  "club" --db-path "club.sql"`.
- **Club picker template** — when login email matches multiple
  tenants, render a "which club?" picker page.
- **Invite URL with tenant slug** — change `/invite/{token}` to
  `/invite/{slug}/{token}` so public invite acceptance resolves the
  correct tenant DB without auth.
- **Role gating on remaining endpoints** — currently only solver
  (Coach+) and /users (PD+) are gated. Per the permission matrix,
  boats/edit should be PD+, sync should be Coach+, rower edits
  should be Coach+.
- **Rower self-service guard rails** — coaches can lock specific
  profile fields so members can't change them (deferred from Phase
  5).

## Suggested next moves

If picking up from a fresh session:

1. **#63** seat locks — foundational, unblocks #61.
2. **#61** no-show handling — lands on top of #63.
3. **#62** manual swap — independent, "commit first, edit committed".
4. **#51** practice notes — quick win.
5. **Remaining role gating** — tighten permissions per the matrix.
6. Then productionization (#55, #56) and polish (#54).
