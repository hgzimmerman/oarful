# lineup_generator — open TODOs

A snapshot of the work backlog as of 2026-04-10. Captures the design
intent behind each pending task so the next session (human or agent)
can resume without re-deriving context.

## Already shipped

- **#45** Sync sheet UI (`POST /sync` form + result panel)
- **#46** Solver knob form on `/solve/{date}` (`SolveKnobs` query
  string + hidden inputs on commit form)
- **#47** Stop double-solving on commit (handled with #46)
- **#48** Pre-solve diagnostics for unsatisfiable lineups (cheap
  eligibility checks surface in the UI banner)
- **#49** Inline rower edits on `/rowers` (HTMX outerHTML swap)
- **#50** Per-rower detail page at `/rowers/{id}` with inline pair +
  seat affinity CRUD
- **#51** Practice notes editor (textarea on history detail + list
  preview, `POST /history/{date}/notes`)
- **#52** `Practice::list_committed` query
- **#53** Alternative-vs-primary diff highlighting on solve view
- **#57** Solver latency measurement → dropped `DEFAULT_BUDGET_SECS`
  from 3 → 1
- **#58** Auth & multi-tenancy (all 5 phases)
- **#59** Global semaphore bounding concurrent solver runs
- **#60** Dedicated rayon thread pool for solver work
- **#61** No-show handling + carry-forward via unified reference
  lineups. "Based on" checkbox list + similarity knob on solve view.
  No-show checkboxes on history detail.
- **#62** Manual rower swap on solve view (Alpine.js click-to-swap,
  direct commit endpoint `POST /commit-lineup/{date}`, bench/sculling
  swap targets)
- **#64** Boat CRUD: list / add / edit / relinquish
- **Role gating** completed per permission matrix (boats PD+, sync
  Coach+, rower edits Coach+, notes Coach+). Renamed `require_role`
  → `require_at_least_role`.
- **Mailer trait** + `LogMailer` for invite delivery. Resend invite
  button + role column on user list.

## Open work

### Coach features

#### #63 — Solver-side seat locks: pre-pin (rower, boat, seat) assignments

Coach use case: "I want Alice in stroke of Persephone, no matter
what — solve the rest around that." No longer blocks #61 (which
shipped via baseline similarity), but still useful on its own.

**Solver work.** Add a `locks: Vec<SeatLock>` field on `SolveRequest`
where each lock is `(RowerId, BoatId, seat_position)`. In the model
build phase, post `x[r, b, s] == 1` + force `use[b] = 1` for each
lock. Validate eligibility before model build with friendly errors
via the `Diagnostic` enum.

**UI.** Lock icon per seat on `/solve/{date}`. Click toggles a lock
and re-solves. Locks visually distinct (different bg, lock icon).

#### Solver presets (profiles)

Preset weight configurations for different coaching scenarios.
Segmented control (button group) on the solve form.

**Built-in presets:**
- **Even speed** — high S1/S9, low S4. Boats stay together.
- **Tiered / coached** — low S1, high S11/S12. Top boat stacked.
- **Balanced** — current defaults.

**Custom profiles.** `solver_profile` table with one typed column
per `SolverConfig` weight (NOT NULL, no defaults). Future migrations
backfill new columns with 0. "Save as preset" button persists
current weights.

No solver changes needed — purely UI + storage.

### Polish

#### #54 — Print-friendly stylesheet for solve / history views

`@media print` rules (or `/print/{date}` route) that hides navbar,
expands alternatives, drops backgrounds, one boat per page-break.

### Productionization

#### #55 — Production static-asset path resolution

Multi-path fallback: `PUBLIC_DIR` env → `exe_dir/public`.

#### #56 — Custom Tailwind build pipeline

Replace CDN with local `tailwind.config.js` scanning
`crates/server/src/**/*.rs` → `crates/server/public/tailwind.css`.

### Parked

#### #48 — Deeper unsat diagnostics (relaxation pass / Pumpkin unsat core)

Pre-solve diagnostics shipped. The deeper relaxation-pass work
(re-solve with each hard constraint disabled to identify the
culprit) remains parked pending a Pumpkin API dive.

## Follow-ups not yet tracked as tasks

- **CLI `create-tenant` command**
- **Club picker template** (multi-tenant login)
- **Invite URL with tenant slug**
- **Rower self-service guard rails** (field locking)

### Per-team roles

Currently roles are global per user. Real-world scenario: a PD
rows on the morning team and coaches the afternoon team.

**Design direction.** `team_membership(user_id, team_id, role)`
replaces `user_role`. Active team determines effective role.
Touches schema, JWT claims, role gating, invite flow, team
switching, user list UI.

Needs more refinement — interaction with multi-tenancy and
migration path from global roles need thought.

## Suggested next moves

1. **#63** seat locks — "pin Alice in stroke" coach use case.
2. Solver presets / profiles (segmented control UI).
3. Productionization (#55, #56) and polish (#54).
