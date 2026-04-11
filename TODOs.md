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
- **Cox position: bow-loader vs stern-loader display order** — per-boat
  `cox_position` enum (Bow/Stern) + tenant-level `force_cox_stern`
  flag. Seats render stern→bow; bow-loaders show cox at the bottom.
- **Show benched/sculling rowers on history detail** — re-derived from
  snapshot by subtracting placed rowers from available. Displayed as
  name lists below the committed lineup cards.
- **Fix bench ↔ boat swaps + pull-to-bench** — rewrote Alpine doSwap
  to use data attributes instead of innerHTML swap (fixes cross-DOM
  corruption). Added "move to bench" action for pulling rowers out
  of seats. Empty seats render as clickable placeholders.
- **Practice scheduling UI** — date picker + "Create" button on
  `/practices`. Coach+ role-gated. Created practices appear in the
  list even before availability is synced.
- **Practice-driven availability** — `/my/availability` now shows
  upcoming scheduled practices with inline status dropdowns.
  Rowers see all practices and can respond per-date without needing
  the free-form date picker (kept as fallback for ad-hoc dates).

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

#### Disambiguate rower attribute labels in lineup cards

"Intermediate" appears in both skill and strength enums, making
the compact stats line ambiguous. Rename "Skill" → "Form" in the
UI (and possibly DB enum) to differentiate. Rename weight class
"Medium" → "Middleweight" for clarity. Then use abbreviated labels
in the stats line: e.g. `Mdl · Int · Int · Port` where the
distinct category names make each value unambiguous. Apply
consistently on solve view seat rows and bench/sculling chips.

#### Bow-loader cox fit penalty

Bow-loader coxed boats (4+s with `cox_position = Bow`) have a
tight bow compartment. Tall rowers are the primary problem —
height is the main constraint on fitting in the space. Weight
matters somewhat too but is secondary.

**Solver work.** In the objective function, add a penalty term
when a rower assigned to the cox seat (position 0) of a
bow-loader boat is tall or heavy. Height-based penalties should
dominate, with weight as a smaller additional factor:

- Tall rower in bow-loader cox: −5
- Very tall: −8
- Heavy rower: −1
- Very heavy: −3
- Short/light: no penalty

This only applies when `boat.cox_position == Bow`. Stern-loader
cox seats have no size constraint.

**No UI changes needed** — purely solver-side scoring.

#### Availability reminder emails

Coach action: select one or more upcoming practice dates and send
a reminder email to all team rowers who haven't responded yet.

**UI.** On `/practices`, checkboxes per date + a "Send reminders"
button (Coach+ gated). Alternatively, a per-date "Remind" button
on each practice row. Shows a confirmation with the count of
recipients before sending.

**Backend.** For each selected date, query rowers on the team who
have no `availability` row for that `(rower_id, team_id, date)`.
Join against `rower` to get email addresses. Call the `Mailer`
trait for each recipient.

**Mailer.** Add a `send_reminder` method to the `Mailer` trait
(alongside the existing `send_invite`). The `LogMailer` impl
logs the reminder the same way it logs invites. The method
signature should take `to_email`, `to_name`, and a list of
practice dates they haven't responded to.

**Email content.** Something like: "Your coach needs your
availability for [dates]. Please respond at [link to
/my/availability]."

No real email provider needed yet — `LogMailer` covers it.
Swap in a real implementation (e.g. Resend, SES) later via
the same trait.

#### Stale lineup detection on availability change

When a rower changes their availability to "No" (or any
non-available status) after a lineup has already been committed
for that date, the committed lineup is now stale — it includes
someone who won't be there.

**Detection.** On the history detail view, cross-reference
committed seat assignments against current availability. Any
rower whose availability is no longer "Yes" gets flagged.

**UI on history detail.**
- Stale lineups get a warning banner: "Availability changed since
  this lineup was committed."
- Affected rowers are highlighted in their seat row (e.g. amber
  background + "availability changed" badge).
- Two actions offered:
  1. "Re-solve" link pre-filled with the committed lineup as
     baseline + similarity prioritized, with the stale rower(s)
     excluded.
  2. Coach can manually swap via the solve view.

**Optional: proactive notification.** When a rower changes
availability for a date that has a committed lineup, surface a
warning in the UI (e.g. a badge on the history nav item or a
toast). Could also trigger a reminder email to the coach. Lower
priority than the detection/display work.

#### Walk-on rower addition from the solve view

A rower shows up to practice without having set their availability
to "present." The coach needs to include them from the solve/edit
view without leaving the page. Two paths after adding:

1. **Manual swap** — the walk-on appears in the bench/sculling
   pool and the coach clicks them into a seat.
2. **Re-solve with walk-on** — the walk-on is added to the
   available set, and the solver re-runs with similarity
   prioritized so existing placements stay stable.

**UI.** A "+ Add rower" button or typeahead on the solve view that
lists roster members not currently marked available. Selecting one
either:
- Adds them to the bench pool (for manual swap), or
- Toggles a checkbox that includes them in the next re-solve.

**Backend.** Temporarily override the rower's availability for
this solve session. Options:
- Patch `DbSnapshot.availability` in the handler before solving
  (no DB write — transient for this request).
- Or upsert a real availability record so it persists (cleaner
  audit trail but more side-effects).

The transient approach is simpler and avoids surprise availability
changes in the spreadsheet sync.

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

### Discord integration (long-term)

Low priority — don't start until most other work is done.

**Minimal (push-only):** A Discord bot that posts committed
lineups to a configured channel when a coach commits. No identity
linking needed — just a webhook or bot token + channel ID in
tenant config. The message would be a formatted lineup card
(boat name, seat assignments, bench/sculling lists).

**Full (bidirectional):** Rowers set availability from Discord
(e.g. reacting to a practice-date message or a slash command).
Requires linking Discord user IDs to app user accounts — either
via an OAuth flow or a manual `/link` command with a one-time
token. Significantly more work and a heavier dependency.

Start with push-only; bidirectional can follow if there's demand.

## Suggested next moves

1. **#63** seat locks — "pin Alice in stroke" coach use case.
2. Solver presets / profiles (segmented control UI).
3. Productionization (#55, #56) and polish (#54).
