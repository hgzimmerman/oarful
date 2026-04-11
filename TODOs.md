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
- **Solver presets + custom profiles** — segmented control with four
  built-in presets (Balanced, Even speed, Tiered, Random) plus
  team-specific custom profiles stored in `solver_profile` table.
  "Save as preset" endpoint persists the current config. Custom
  profiles appear alongside built-ins in the selector (violet).
- **Manual lineup builder** — boat selector + empty boat cards + rower
  pool on the landing page. Coach can place rowers by hand, commit
  directly, or click Generate to let the solver fill the rest
  (placements become seat locks automatically).
- **Walk-on rower addition** — "+ Add walk-on" dropdown on the solve
  landing page listing unavailable roster members. Selecting one adds
  a transient availability override (no DB write). Walk-ons appear in
  the available pool for both manual builder and solver. Carried as
  `walkon` query params across re-solves.
- **Stale lineup detection** — history detail cross-references committed
  seats against current availability. Warning banner + amber highlight
  with "unavailable" badge on rowers whose status changed.
- **#63 Solver-side seat locks** — `SeatLock` struct on `SolveRequest`,
  posts `x[r,b,s]=1` + `use[b]=1` per lock. Invalid locks surfaced
  as `Diagnostic::InvalidLock` and skipped. UI: lock/unlock icons per
  seat row, violet highlight on locked seats, locks carried as query
  params and hidden form inputs across re-solves.
- **Bow-loader cox fit penalty (S14)** — penalises tall/heavy rowers
  in bow-loader cox seats. Height: Tall +3, VeryTall +5. Weight:
  Heavy +1. Stern-loaders unaffected. Configurable via
  `bow_cox_fit_weight` in SolverConfig.
- **Disambiguate rower attribute labels** — "Skill" renamed to "Form"
  in UI labels. Weight class shows Lightweight/Middleweight/Heavyweight.
  Compact stats line uses abbreviated labels (Lt/Md/Hv · Nov/Int/Mst/Exp
  · Wk/Int/Str/V.Str · Side). DB enum values unchanged.
- **Practice-driven availability** — `/my/availability` now shows
  upcoming scheduled practices with inline status dropdowns.
  Rowers see all practices and can respond per-date without needing
  the free-form date picker (kept as fallback for ad-hoc dates).

## Open work

### Coach features

#### Smart boat pill interactions

Boat pills in the lineup editor have context-dependent behavior
based on selection state:

**Click an unselected pill (no prior selection):**
- Adds an empty boat card to the lineup. No rowers moved.

**Click a live (populated) pill (no prior selection):**
- Unloads the boat — all rowers/cox return to the pool. Card
  hidden.

**Select a live pill, then click an unselected pill:**
- Swap/transfer: move rowers from the live boat into the new
  boat. Seat mapping by position:
  - Stern pair stays stern, bow pair stays bow, cox stays cox.
  - If resizing (8→4): middle seats go to bench.
  - If resizing (4→8): middle seats left empty.
  - If rigging differs (port↔starboard): swap pairs within
    their partition so rowers stay on their preferred side.
  - Original boat is unloaded after transfer.

**Select a live pill, then click another live pill:**
- Swap all rowers between the two boats (or swap their visual
  positions). This may need an undo button since it's a large
  change.

**Edge cases:**
- Coxed ↔ coxless: cox goes to bench on downgrade, empty cox
  seat on upgrade.
- Very different sizes (2→8): only bow pair transfers, rest
  empty.

Implementation: client-side in Alpine. Boat pills get a
"selected" state (like rower selection) with a visual indicator.
Second click resolves the action.

#### Proactive stale lineup notification

When a rower changes availability for a date that has a committed
lineup, proactively surface a warning — e.g. a badge on the
history nav item, a toast on next page load, or a reminder email
to the coach. Lower priority than the detection/display (which
shipped), but would close the feedback loop so the coach doesn't
have to manually check the history page.

#### Batch invite from roster

The coach syncs a sheet → rowers appear on the roster with emails
but no user accounts. Currently each rower must be individually
invited from the Users page. Need a bulk flow:

- A "Send invites" button on the roster page (Coach+ gated) that
  creates AppUser accounts + sends invites for all roster members
  who have an email but no linked user account.
- Show invite status per rower on the roster (uninvited / pending /
  active) so the coach can see who still needs an invite.
- Use the existing Mailer trait (LogMailer for dev).

#### Self-edit trust levels — UI for team config

Self-edit trust levels are implemented (low/medium/high on the
team table). Need a UI for PDs to change the setting — currently
requires direct DB edit. Could go on the future team management
page.

#### Email visibility tenant config

Some teams want email addresses visible on the roster list and
detail pages; others consider them private. Add a tenant-level
boolean `emails_visible` (default false). When true, the roster
list shows an Email column and the detail page shows the email.
When false, emails are hidden from the UI for regular members
but still visible to Coach+ and PD roles (they need them for
invites and communication). Always used internally for invites
and sync matching regardless of visibility setting.

Follows the same pattern as `attributes_public` — cached in
TenantConfig, threaded through TenantContext. The visibility
check combines the tenant flag with the user's role.

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


### Productionization

#### #55 — Production static-asset path resolution

Multi-path fallback: `PUBLIC_DIR` env → `exe_dir/public`.

#### #56 — Custom Tailwind build pipeline

Replace CDN with local `tailwind.config.js` scanning
`crates/server/src/**/*.rs` → `crates/server/public/tailwind.css`.

### Infrastructure

#### Persistent sync URL + periodic polling

The Google Sheet sync currently requires the coach to paste the
sheet ID and tab ID every time. Save the sync configuration so
it can be re-used and optionally polled automatically.

**Saved config.** Store `sheet_id` and `tab_id` (GID) on the
team (or a `sync_config` table keyed by team). The sync page
pre-fills from the saved config. Updating the fields overwrites
the saved config.

**Manual re-sync.** A one-click "Re-sync" button on `/sync`
that uses the saved config without re-entering IDs.

**Periodic polling.** An optional background task that re-syncs
on a schedule (e.g. every 30 minutes, configurable). Only runs
if a sync config exists for the team. Uses the same sync logic
as the manual flow. Logs results; surfaces last-sync timestamp
and any errors on the sync page.

**Considerations:**
- Polling requires a background task runtime (tokio::spawn with
  a timer, or a cron-like scheduler). Keep it simple — a loop
  with `tokio::time::interval` is sufficient.
- Rate-limit to avoid hammering Google's export endpoint.
- The sync page should show last-sync time and status (success /
  error / never synced).

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

#### Boat usage tracking from committed lineups

The sibling `boat_tracking` project tracks boat uses for
depreciation/maintenance purposes. Rather than maintaining a
separate logging workflow, derive boat usage from committed
lineups — once a practice date has passed, each committed lineup
counts as one use of that boat.

**Approach:**
- Query committed lineups with `practice.date < today`, grouped
  by `boat_id`, to get a usage count and usage history.
- Surface on the boat detail page: total uses, last used date,
  usage over time (simple count, not a chart initially).
- Salvage any useful schema or logic from `boat_tracking` (lives
  in the adjacent directory) — particularly maintenance intervals,
  damage reporting, or depreciation formulas if they exist.

**No new data entry needed** — the lineup commit flow already
records which boats were used on which dates. This is purely a
read-side feature built on existing data.

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

### Self-service team onboarding + business model

**Business model.** The software is FOSS (AGPL-3.0 — copyleft
covers network use, appropriate for a hosted SaaS). Revenue comes
from a flat yearly hosting fee per tenant. Custom feature work
available at additional cost. Deployment config (k3s/k8s) is
private — not part of the open-source release.

**Self-service onboarding.** A public signup flow where a new
club can create their own tenant without manual intervention:

1. Landing/marketing page explaining the product + pricing.
2. "Get started" → registration form: club name, admin email,
   admin name, password. Creates a tenant + PD user in one step.
3. Tenant gets its own SQLite file, seeded with an empty roster
   and the PD account. The PD can then invite coaches and rowers.
4. Payment integration (Stripe or similar) — gate tenant creation
   behind payment, or allow a trial period then require payment
   to continue. Details TBD.

**Schema.** The `tenant` table already exists. Onboarding creates
a new row + a new SQLite file. May want a `billing_status` field
(trial / active / suspended / cancelled) and `trial_expires_at`
on the tenant to gate access.

**Payment.** Integrate a payment processor (Stripe likely).
Tenant table gets `billing_status` (trial / active / suspended /
cancelled), `trial_expires_at`, and a `stripe_customer_id` (or
equivalent) to link tenants to their payment state. Middleware
checks billing status on each request — suspended tenants see a
"renew your subscription" page instead of the app.

**CI/CD.** Build/test/lint pipelines live in this repo (GitHub
Actions or similar). Deployment pipelines (k3s/k8s rollout) live
in the separate private infra repo.

**Licensing.** AGPL-3.0 LICENSE file and copyright notice shipped.
The deployment manifests (k3s/k8s YAML, infrastructure-as-code)
live in a separate private repo.

### Demo mode

Self-service demo for prospective users — try the app without
signing up for a real account. The "Try demo" button lives on the
landing/marketing page (from the onboarding TODO above) so
prospects can kick the tires before committing to a paid plan.

**Ephemeral tenants.** Clicking "Try demo" creates a new tenant
with a pre-seeded fixture (toy rowers, boats, a few practices
with availability). The tenant gets a random slug and an
auto-logged-in session (skip invite/registration flow).

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

### Streaming alternatives (stretch goal)

Return the primary lineup immediately and stream alternatives
as they complete, rather than blocking until all are done.

**HTMX 2.0 (current):** Feasible via the SSE extension
(`htmx-ext-sse`). Requires a separate SSE connection — the
initial request returns the primary lineup + an SSE connect
div. A background solver task feeds alternatives through a
channel; the SSE endpoint streams them as HTML fragments.

**HTMX 4.0 (early-mid 2026):** Native streaming via
`fetch() + ReadableStream`. Any `text/event-stream` response
works without an extension. Also enables POST-based SSE.
Migration from 2.0 SSE extension would be a simplification
(drop extension, keep same SSE endpoint).

**Chunked transfer:** Not viable on HTMX 2.0 (XHR buffers
the whole response). HTMX 4.0's fetch migration fixes this.

**Backend.** The solver already produces alternatives
sequentially (tabu re-solve loop). Wrap in an async channel:
send primary immediately, then each alternative as it
completes. The handler becomes an SSE endpoint that yields
events from the channel.

**UI.** The alternatives panel renders a "Computing
alternatives..." placeholder, then each alternative card
appears as it arrives via SSE swap.

**Recommendation:** Implement on HTMX 2.0 SSE extension now.
Migrate to HTMX 4.0 native streaming when it stabilizes.
Low priority — only matters when alternatives > 0 and the
per-alternative budget is long enough to notice the delay.

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
