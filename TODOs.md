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

#### Manual lineup builder + partial seed → solver

The solve view currently requires running the solver to produce a
lineup. Coaches should be able to build lineups entirely by hand,
or seed a partial lineup and let the solver fill the rest.

**Manual-only flow:**
1. Coach selects which boats to field (checkbox list of available
   boats on the solve landing page).
2. For each selected boat, an empty boat card appears with
   clickable empty seats.
3. Coach clicks a seat, then clicks a rower from the available
   pool to place them. The available pool shows all rostered
   rowers for the date.
4. Coach can commit the lineup as-is (partially or fully filled)
   without ever running the solver.

**Partial seed → solver flow:**
1. Coach manually places a few key rowers (e.g. "Alice in stroke
   of Persephone, Bob coxing Artemis").
2. Coach clicks "Generate" — the solver treats manually placed
   rowers as locked seats (#63 seat locks) and fills the rest.
3. The solver also respects the coach's boat selection: only
   boats the coach selected (or that have seed placements) are
   candidates.

**Interaction with seat locks (#63):** The manual placements
become the locks passed to `SolveRequest.locks`. This means #63
is a prerequisite for the seed→solver flow, but the manual-only
flow (no solver) can ship independently.

**UI considerations:**
- The solve landing page needs a boat selector (multi-select or
  checkbox list of in-service sweep boats).
- Empty boat cards need the same swap interaction as filled ones
  (click seat → click rower from pool).
- The "Generate" button should be optional — "Commit lineup" works
  without solving if the coach is happy with manual placements.

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

#### Mobile-responsive pass

In practice, coaches use this on their phone at the boathouse.
The current layout is desktop-first — multi-column grids, wide
tables, and inline forms that don't work well on narrow viewports.

**Priority order (by real-world usage):**
1. **Solve view** — the most critical mobile screen. Boat cards,
   seat rows, swap interactions, knobs form, and the bench/sculling
   pool all need to work in a single-column touch-friendly layout.
2. **History detail** — reviewing committed lineups + marking
   no-shows on the go.
3. **Practices list** — picking a date to solve.
4. **`/my/availability`** — rowers responding from their phone.
5. **Rower list + detail** — less urgent but still used.

**Approach:**
- Tailwind responsive utilities (`sm:`, `md:` breakpoints) on
  existing classes — most of the grid/flex layouts just need
  single-column fallbacks.
- Boat cards: stack to full-width on small screens.
- Knobs form: stack inputs vertically instead of 5-column grid.
- Tables: consider horizontal scroll or card-based layout for
  narrow screens.
- Swap interactions: ensure touch targets are large enough
  (minimum 44px) and the selection hint is visible without
  scrolling.
- Test on 375px width (iPhone SE) as the baseline.

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

### Infrastructure

#### Audit log

Track who changed what and when. Useful for coaches reviewing
availability changes, PDs auditing role grants, and debugging
unexpected state.

**Schema.** A single `audit_log` table in the per-tenant DB:
- `id`, `timestamp`, `user_id` (nullable — system actions have
  no user), `action` (enum or free text: e.g. "availability.update",
  "lineup.commit", "rower.update", "invite.create"),
  `resource_type`, `resource_id`, `detail` (JSON blob with
  before/after or relevant context).

**Write points.** Instrument handlers that mutate state:
availability upserts, lineup commits, rower edits, role changes,
invite creation, boat CRUD, practice creation, notes updates.

**Read UI.** A filterable log view for PD+ (by resource, by user,
by date range). Lower priority than the write instrumentation —
the data is valuable even before there's a dedicated UI (queryable
via SQLite directly).

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

### Demo mode

Self-service demo for prospective users — try the app without
signing up for a real account.

**Ephemeral tenants.** A "Try demo" button on the login page
creates a new tenant with a pre-seeded fixture (toy rowers, boats,
a few practices with availability). The tenant gets a random slug
and an auto-logged-in session (skip invite/registration flow).

**Lifecycle.** Ephemeral tenants are tagged with a `demo_expires_at`
timestamp (e.g. 1 week from creation). A background cleanup job
(or startup sweep) deletes expired demo tenants and their SQLite
files.

**Constraints:**
- Demo tenants always use `LogMailer` — no real emails sent.
- Rate-limit demo creation (e.g. by IP or a simple cooldown) to
  prevent abuse.
- Demo tenants are read-write (coaches can solve, commit, edit)
  but isolated — no cross-tenant visibility.

**Schema.** Add `demo_expires_at` nullable timestamp to the
`tenant` table. Non-null means ephemeral; null means permanent.

Don't start until most features are stable — the demo should
showcase a polished product.

### Regatta lineup generator (long-term)

A significantly more complex solver mode for race-day scheduling.
Unlike practice lineups (one set of boats, one time slot), regattas
involve multiple races across a day with shared resources.

**Core constraints:**

- **Race schedule.** Each race has a fixed start time and a
  category (e.g. Mens 4+, Womens 8, Mixed 4x). The solver must
  assign boats and rowers to races respecting category rules.

- **Boat reuse.** The same boat can race multiple times per day,
  but not simultaneously. A boat finishing at 10:15 can't start
  another race until some turnaround time has elapsed (getting
  from finish line → dock → start line).

- **Trailer capacity.** The club can't bring the whole fleet.
  The solver must work within a declared subset of boats that
  fit on the trailer. This is a hard constraint input, not
  something the solver decides.

- **Multi-race rowers.** Some rowers opt into rowing more than
  once per day. Rowers who haven't opted in are single-race
  only. Opted-in rowers still need minimum turnaround time
  between races (dock swap time).

- **Cooldown preferences.** Beyond the hard minimum turnaround,
  some rowers prefer longer rest between races. Model as a soft
  penalty — the solver tries to respect it but can override if
  necessary for feasibility.

**Gender and category rules:**

- Rowers have an optional `racing_gender` (Man/Woman) on the
  rower table. Required to be eligible for regatta placement —
  rowers without it are excluded from the regatta solver.
  Non-binary rowers declare one for racing purposes (organizer
  requirement). Separately, add optional `pronouns` (free text)
  on the rower table for display purposes — not used by solver.
- Mens races: men only.
- Womens races: women only. Men cannot row in womens races.
- Womens-open toggle: allow women in mens boats (typically
  discouraged — model as a penalty, not a hard block, with a
  coach toggle to forbid it entirely).
- Mixed races: target equal men/women split in the boat. More
  women than men is allowed; more men than women is not. Model
  the equal-split target as a soft goal, with the women≥men
  ratio as a hard constraint.

**Data model additions:**

- `regatta` table: name, date, trailer boat list. A regatta
  can draw rowers from multiple teams within the tenant (e.g.
  mens + womens teams both attend the same regatta). Model as
  a many-to-many `regatta_team` join table. The eligible rower
  pool is the union of all linked teams' rosters.
- `race` table: regatta_id, category, start_time, boat_class
  (4+, 8, etc.).
- `race_entry` table: race_id, boat_id, seat assignments.
- `regatta_attendance` table: regatta_id, rower_id,
  `max_races` (how many races this rower is willing to do at
  this regatta), `preferred_cooldown_minutes` (soft rest
  preference between races at this regatta). These vary per
  regatta, not per rower globally.
- Rower table additions: `racing_gender` (nullable enum
  Man/Woman), `pronouns` (nullable text).
- Team config: `default_racing_gender` (nullable) on the `team`
  table. When set, pre-fills the racing gender field on new
  rower creation and the self-service profile form. Simplifies
  setup for single-gender teams (e.g. a mens team doesn't need
  every rower to manually select "Man").

**Solver approach:** This is a much harder combinatorial problem
than practice lineups — it's essentially a resource-constrained
scheduling problem with gender constraints. May need a different
solver formulation (e.g. time-indexed variables, or a two-phase
approach: assign rowers to races first, then schedule boats).

Very large scope — park until practice lineups are mature and
there's real demand from clubs doing regattas.

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
