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
- **Side preference visual indicator** on seat rows (colored bar +
  opacity scaling with side_strength)
- **Visual distinction for designated coxswains** (border + "COX"
  chip on seat rows)
- **Boats page layout** — "Add boat" button moved inline with header
- **Fix default stroke side** — new boat form defaults to Port
- **Show full rower cards for bench/sculling** on solve view (compact
  stats line on unplaced rower chips)
- **Lazy solve landing page** — `/solve/{date}` renders knobs first,
  solver runs on explicit Generate click
- **Fix self-service nav** — dev user linked to toy rower in fixture
- **Per-request tracing + handler instrumentation** — TraceLayer
  middleware with request ID, method, path, user/team ID spans;
  `#[tracing::instrument]` on handlers
- **Role-gate rower attribute visibility** — tenant-level
  `attributes_public` flag (default private); Coach+ always sees
  full stats, Members see only side preference unless tenant opts in
- **Rower attribute editing on detail view** — moved from inline list
  row edit to HTMX section swap on `/rowers/{id}`. List view is now
  read-only with clickable name linking to detail page.

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

#### Show benched/sculling rowers on history detail view

The history detail page only shows committed lineups (who's in a
boat). It doesn't show who was available but didn't get a seat —
the benched and sculling-redirect rowers are invisible. Add a
section similar to the solve view's unplaced block so the coach
can see the full picture of who was there that day.

Requires either storing unplaced rowers at commit time (new table
or JSON column) or re-deriving them from the snapshot (load
availability for the date, subtract placed rowers).

#### Disambiguate rower attribute labels in lineup cards

"Intermediate" appears in both skill and strength enums, making
the compact stats line ambiguous. Rename "Skill" → "Form" in the
UI (and possibly DB enum) to differentiate. Rename weight class
"Medium" → "Middleweight" for clarity. Then use abbreviated labels
in the stats line: e.g. `Mdl · Int · Int · Port` where the
distinct category names make each value unambiguous. Apply
consistently on solve view seat rows and bench/sculling chips.

#### Cox position: bow-loader vs stern-loader display order

Coxes in 8s sit behind stroke (seat 8), but coxed 4s can be
bow-loaders (cox at bow, ahead of seat 1) or stern-loaders (cox
behind stroke). Currently the cox (seat 0) is always displayed
first, which is wrong for stern-loaders.

**Schema change**: add `cox_position` enum (`Bow` | `Stern`) to
the `boat` table. Default `Stern` for 8s, `Bow` for 4+s (but
editable per boat).

**Display**: render seats top-to-bottom as stern → bow. For a
stern-loader: cox, s8, s7, ..., s1. For a bow-loader: s8, s7,
..., s1, cox. This matches the physical layout of the boat.

**Solver**: no impact — seat 0 is still the cox seat regardless
of physical position. This is purely display ordering.

#### Fix bench ↔ boat swaps + support pulling rowers to bench

Two issues with the manual swap UI:

1. **Bug**: swapping a benched/sculling rower with a seated rower
   doesn't work. The Alpine `doSwap` swaps `.rower-content`
   innerHTML between the two elements, but seated rowers are `<tr>`
   table rows and bench rowers are `<span>` pills — the DOM
   structures don't match, so the swap silently fails or corrupts
   the display. Fix: make `doSwap` aware of the element types and
   move content correctly between table rows and pill elements.

2. **Feature**: allow pulling a rower out of a boat to the bench
   without replacing them, leaving an empty seat. Currently swaps
   require two rowers. Options:
   - A "bench" action per seat (small X button) that moves the
     rower to the bench list and renders the seat as empty.
   - Or let the coach click a seated rower then click a "Bench"
     target area (not a specific benched rower) to pull them out.
   - Empty seats should be visually distinct and clickable as swap
     targets so a benched rower can be placed into them.

#### Practice scheduling UI

There's no way to create upcoming practices from the web UI. The
`/practices` page shows dates derived from availability data, but
coaches can't schedule a new practice date. Need:

- A way to create a practice for a future date (simple date
  picker + "Create practice" button on `/practices`)
- Possibly recurring schedule support (e.g. "every Tuesday and
  Saturday") so coaches don't have to create each date manually
- Created practices should appear on `/practices` and be available
  for rowers to set availability against

Currently practices are implicitly created when availability is
synced from the Google Sheet or when a lineup is committed. An
explicit scheduling step would let the system drive the
availability-collection workflow instead of relying on the sheet.

#### Team management UI for Program Directors

There's no way for a PD to create, view, or manage teams in the
web UI. Teams exist in the DB and the navbar has a team selector,
but there's no CRUD page. Need:

- `GET /teams` — list all teams in the tenant
- `GET /teams/new` + `POST /teams` — create a team
- `GET /teams/{id}` — view team with its roster (members)
- Ability to add/remove rowers from a team
- Role-gated to ProgramDirector+

### Polish

#### #54 — Print-friendly stylesheet for solve / history views

`@media print` rules (or `/print/{date}` route) that hides navbar,
expands alternatives, drops backgrounds, one boat per page-break.

### Observability

#### Full invite URL using ORIGIN env var

The invite URL is just a path (`/invite/{token}`). Both the mailer
and the invite result UI page display it without the origin, making
it unclickable / uncopyable as a real link.

Add an `ORIGIN` env var (e.g. `http://localhost:3000`). Store it on
`AppState`. The invite handler prepends it to produce a full URL
for both the mailer call and the UI result page display.

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
