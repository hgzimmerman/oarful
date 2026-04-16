# lineup_generator — open TODOs

A snapshot of the work backlog as of 2026-04-16. Captures the design
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
- **#63** Solver-side seat locks — `SeatLock` struct on `SolveRequest`,
  UI lock/unlock icons, violet highlight, locks carried as query params.
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
  snapshot by subtracting placed rowers from available.
- **Fix bench ↔ boat swaps + pull-to-bench** — rewrote Alpine doSwap
  to use data attributes instead of innerHTML swap.
- **Practice scheduling UI** — date picker + "Create" button on
  `/practices`. Coach+ role-gated.
- **Solver presets + custom profiles** — segmented control with four
  built-in presets plus team-specific custom profiles.
- **Manual lineup builder** — boat selector + empty boat cards + rower
  pool on the landing page.
- **Walk-on rower addition** — "+ Add walk-on" dropdown on the solve
  landing page.
- **Stale lineup detection** — history detail cross-references committed
  seats against current availability. Warning banner + amber highlight.
- **Bow-loader cox fit penalty (S14)** — penalises tall/heavy rowers
  in bow-loader cox seats.
- **Disambiguate rower attribute labels** — "Skill" renamed to "Form"
  in UI labels.
- **Practice-driven availability** — `/my/availability` shows upcoming
  scheduled practices with inline status dropdowns.
- **Coach email blast** — all 4 phases shipped. Magic links, email
  opt-out, HTML email templates, practices page tabs with send UI.
- **Club picker (multi-tenant login)** — two-step login with club
  picker for multi-tenant users.
- **Magic-link sign-in** — "Email me a sign-in link" for returning
  users.
- **Zone-based seat affinities** — replaced absolute seat positions
  with zone-based system (SeatZone).
- **Audit log** — `audit_log` table with fire-and-forget writes,
  90-day retention cleanup, PD-only filterable viewer at `/audit`.
- **Batch invite from roster** — Coach+ bulk-creates Member accounts
  + sends invite emails.
- **Email→app_user refactor** — moved email ownership from `rower` to
  `app_user`, reversed FK direction.
- **Sweep bias** — replaced boolean `can_scull` with `sweep_bias`
  integer (-2..2) on rower.
- **Scull boat support in solver** — solver places rowers in both
  sweep and scull boats in a single pass.
- **Sync row filter** — `RowFilter` enum (All/Sweep/Sculling) on sync
  source config.
- **Practice duration + cross-team overlap detection** —
  `default_practice_duration_minutes` on team, `duration_minutes` on
  practice, `Practice::find_overlapping()`.
- **Cross-team coordination in editor** — "Available from other teams"
  section + boat-in-use warnings.
- **Soft-delete rowers + archive teams** — rower `active` flag, team
  `archived` flag, PD-only toggles.
- **CLI commands** — `reset-tenant`, `reset-all`, `seed`.
- **Boat form UX** — stroke side hidden for scull boats, cox position
  hidden for coxless boats.
- **Smart boat pill interactions** — boat-to-boat transfer with
  rigging-aware seat mapping + bench overflow.
- **Self-edit trust levels** — low/medium/high on team, PD toggle on
  team detail page.
- **Team management** — Team CRUD, roster view, admin roster matrix.
- **Mobile-responsive pass** — grid collapses, touch targets, hamburger.
- **#54** Print-friendly stylesheet.
- **#55** Production static-asset path resolution.
- **Practice datetime migration** — `time` + `duration_minutes` columns.
- **Periodic sync polling** — background re-sync on schedule.
- **Boat usage tracking** — usage stats from committed lineups, boat
  detail page, fleet CSV export.
- **Data export + backup/import** — PD-accessible SQLite download +
  CLI import command + restore flow with credential checking.
- **Demo mode** — ephemeral tenants, auto-login, 7-day expiry.
- **Sync-created practices inherit team defaults** — time + duration
  applied from team config when sync creates new practices.
- **Default practice days** — `PracticeDays` bitmask on team, date
  picker auto-suggests the next unfilled day.
- **Styled confirmation modals** — replaced all native `confirm()`
  with HTMX-driven styled modals (archive team, deactivate rower,
  delete preset, restore backup).
- **Reminder preview modal** — per-practice checkboxes on Planning
  tab, preview modal lists recipients by name before sending.
- **Lineup preview modal** — preview modal with recipient list and
  scope selection (placed+bench / all) before sending lineup emails.
- **Email visibility tenant config** — `emails_visible` on tenant,
  roster Email column + detail page email, admin Settings tab.
- **Admin Settings tab** — tenant-level toggles for attributes_public,
  emails_visible, force_cox_stern.
- **Global error toasts** — `ErrorResponse` struct with plain-text
  messages, `htmx:beforeSwap` listener shows toasts for all non-2xx.
- **Practices tab fixes** — shared `tab_swap` pattern for active state
  + URL, full-page rendering on direct navigation.
- **History page layout** — Edit/Cancel buttons in header bar.
- **Single-seat zone affinity boost** — Stroke/Bow zones get 2×
  weight to prevent soft constraint outbidding.
- **#56** Custom Tailwind build pipeline — pre-built CSS replaces CDN,
  pre-commit hook regenerates on commit, `tailwindcss` in flake.
- **Streaming alternatives via SSE** — primary lineup streams
  immediately, alternatives append as they complete. `solve_streaming`
  in solver crate, SSE handler, HTMX SSE extension.
- **User account status toggle** — active ↔ disabled on admin Users tab.
- **Solver tuning** — bumped pair_affinity_weight (3→4),
  seat_affinity_weight (3→5), weight_class_slack_weight (1→3),
  default budget (3s→5s) for better real-world results.
- **Handler/template refactoring** — split practices, rowers, fixture
  into submodules. Removed all section separator comments.

## Open work

### Coach features

#### Stale lineup notification — coach-facing

Rower-side detection shipped (warning on the availability page when
a change affects a committed lineup). What's missing is the coach
side: the coach should know without checking the history page.

- **Nav badge:** a small dot or count on the history/practices nav
  item when any committed lineup has become stale since the coach
  last viewed it. Requires tracking "last seen" per coach or a
  simple `stale_since` timestamp on the lineup.
- **Toast on next page load:** when the coach loads any page, check
  for stale lineups and show a dismissable banner ("1 committed
  lineup has availability changes").
- **Email to coach:** optional — send an email when a rower changes
  availability for a committed lineup. Gated by an opt-in flag on
  the coach's account (avoid spam for frequent changes).

### Coach features (continued)

#### Raw rower metrics + team-defined bucketing

Add optional raw numeric fields to rower:
- `weight_lbs: Option<WeightLbs>` — body weight in pounds (newtype over `i32`)
- `height_in: Option<HeightInches>` — height in inches (newtype over `i32`, displayed as `X'Y"`)
- `erg_2k_cs: Option<Erg2kTime>` — 2k erg time in centiseconds (newtype over `i32`, displayed as `M:SS.dd`)

Teams define their own threshold mappings from raw values to the
existing categorical buckets:
- Weight: Lightweight/Middleweight/Heavyweight (from `weight_lbs`)
- Height: Short/Medium/Tall/VeryTall (from `height_in`)
- Strength: Weak/Intermediate/Strong/VeryStrong (from `erg_2k_cs`)
- Skill/Form: stays as a manual coach-set enum (not quantifiable)

Stored as team-level config — e.g. "lightweight < 150 lbs,
middleweight 150–185, heavyweight > 185", "short < 66in, medium
66–71, tall 71–75, very tall > 75".
Raw values are editable by rowers (subject to self-edit trust level);
bucket boundaries are Coach+ only.

**Newtypes:** `WeightLbs(i32)` with `Display` rendering as pounds,
`HeightInches(i32)` with `Display` rendering as feet/inches
(e.g. 71 → `5'11"`), `Erg2kTime(i32)` with `Display` rendering
centiseconds as `M:SS.dd` (e.g. 42350 → `7:03.50`). All get
`DieselNewType` for column mapping.

**UI:** Raw values shown on rower detail page alongside the derived
bucket. Team settings page gets threshold config per metric. Auto-
derive buckets on save when raw values are present and thresholds
are configured.

### Refactor: extract AppState sub-states via FromRef

`State<AppState>` is used by ~20 handlers but each only needs a
subset. Extract logical groupings with `FromRef<AppState>` impls
so handlers declare exactly what they need:

- **`MailerCtx`** (`mailer` + `origin`) — sending emails with full
  URLs. Includes `full_url()`. Used by reminders, lineups, invites,
  magic link, auth.
- **`SolverCtx`** (`solver_pool` + `solve_semaphore`) — dispatching
  solver work. Used by stream handler.
- **`TenantDb`** (`master_db` + `tenant_cache` + `data_dir`) — tenant
  resolution, cache eviction, export/restore. Methods: `tenant_db()`,
  `evict_tenant()`, `tenant_db_by_slug()`.
- **`JwtKeys`** — already a standalone struct, just needs `FromRef`.

`AppState` stays for construction and the auth middleware (which
touches multiple sub-states). Handlers change from
`State(state): State<AppState>` to e.g. `State(mailer): State<MailerCtx>`.

### Parked

#### #48 — Deeper unsat diagnostics (relaxation pass / Pumpkin unsat core)

Pre-solve diagnostics shipped. The deeper relaxation-pass work
(re-solve with each hard constraint disabled to identify the
culprit) remains parked pending a Pumpkin API dive.

## Follow-ups not yet tracked as tasks

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

**Source link.** AGPL requires making source available to network
users. Add a link to the hosted repo (GitHub/etc.) somewhere in
the UI — footer, about page, or login page. Blocked on hosting
the repo publicly.

**Licensing.** AGPL-3.0 LICENSE file and copyright notice shipped.
The deployment manifests (k3s/k8s YAML, infrastructure-as-code)
live in a separate private repo.

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

### HTMX 4.0 SSE migration

When HTMX 4.0 stabilizes (expected mid-2026), migrate the streaming
alternatives from `htmx-ext-sse` to native fetch streaming. HTMX 4.0
uses `fetch()` + `ReadableStream` natively — no extension needed.
The SSE endpoint stays the same; the frontend drops `hx-ext="sse"`
and `sse-connect`/`sse-swap` in favor of native HTMX streaming
attributes. Also enables POST-based SSE (currently requires GET).
This would let us remove `htmx-ext-sse.js` from the public assets.

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

1. **Stale lineup notification (coach-side)** — nav badge or toast.
2. **Raw rower metrics + team-defined bucketing** — weight/erg newtypes + threshold config.
3. **Per-team roles** — design + migration.
