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
- **AppState sub-state extraction via FromRef** — `MailerCtx`,
  `SolverCtx`, `TenantDb`, `JwtKeys` sub-states with `FromRef<AppState>`
  impls. Handlers narrowed: solve→`SolverCtx`, email→`MailerCtx`,
  admin→`TenantDb`, invite→`MailerCtx`/`TenantDb`. Auth middleware +
  demo + sync stay on full `AppState`.
- **Solver tuning** — bumped pair_affinity_weight (3→4),
  seat_affinity_weight (3→5), weight_class_slack_weight (1→3),
  default budget (3s→5s) for better real-world results.
- **Handler/template refactoring** — split practices, rowers, fixture
  into submodules. Removed all section separator comments.
- **Self-service club onboarding** — "Oarful" branding, public landing
  page, signup flow (club name → tenant + PD user + SQLite file),
  30-day free trial with billing_status/trial_expires_at on tenant,
  billing middleware (expired/suspended → "renew" page), AGPL source
  link footer (SOURCE_URL env var). `grandfathered` billing status
  for permanently free early-adopter tenants. `set-billing` CLI command.
- **Default availability assumption** — per-team `assume_available`
  toggle on team settings. Rowers with no response treated as available
  when enabled. Propagated through `DbSnapshot`, solver, editor, and
  history stale-detection.
- **Unsubscribe links in emails** — HMAC-signed stateless tokens in
  reminder/lineup email footers. Per-type and unsubscribe-all links.
  GET confirmation page + POST for RFC 8058 `List-Unsubscribe-Post`.
- **Password reset flow** — "Forgot password?" on login page sends a
  1-hour magic link to `/reset-password`. Dedicated email template,
  new-password form, redirect to login with success banner. E2e tested.
- **Coach-editable availability** — attendance grid cells are now
  tap-to-cycle (Yes → No → clear) for Coach+. HTMX `outerHTML`
  swap per cell. Touch-guard script suppresses clicks during swipe
  scrolling (>10px movement threshold). Audit trail records
  `"set_by": "coach"`.
- **Practice time in emails** — reminder and lineup emails now show
  the practice start time alongside the date (e.g. "Monday — 2026-04-21
  at 6:30 AM"). Threaded through `ReminderRecipient`, `EmailLineupSummary`,
  `Mailer` trait, and email templates.
- **Gate backup restore behind billing status** — trial tenants
  blocked from restore. Active and grandfathered allowed.
- **Email restrictions for trial tenants** — trial/demo tenants
  cannot send outbound email (reminders, lineups, invites). Auth
  emails (magic login, password reset) still work. Reminder/lineup
  handlers return a user-facing message; invite handlers silently
  skip the email but still create the invite record.
- **Invite URL with tenant slug** — new invite URLs use
  `/invite/{slug}/{token}` for direct tenant resolution. Old
  `/invite/{token}` still works (scans all tenants as fallback).
- **Boat usage matrix CSV export** — `GET /boats/usage-matrix.csv`
  exports a boat × date matrix (1 = used, empty = not used) from
  committed lineups. "Usage matrix" button on fleet page (PD+).
- **Stale lineup nav badge (coach-side)** — amber count badge on
  Practices nav link when upcoming committed lineups have availability
  changes. `GET /nav/stale-badge` endpoint, HTMX on-load pattern.
- **Raw rower metrics (Phase 1)** — `weight_kg` (float) and
  `height_m` (float) on rower table, displayed as lbs and
  feet/inches. `erg_test` table with `(rower_id, distance_m,
  time_cs, rowed_at, created_at)` log. Erg test CRUD on rower
  detail page (Coach+). Split /500m displayed alongside time.
- **Rower self-service guard rails** — replaced `SelfEditLevel`
  (Low/Medium/High) with two orthogonal team-level controls:
  `bucket_visibility` (off/view/edit) controls whether members see
  categorical bucket labels; `member_raw_metrics` (bool) controls
  whether members can input weight, height, and erg tests. Members
  can add but not delete erg tests. `POST /my/erg-test` endpoint.
- **Stale lineup email alerts (coach-facing)** — background poller
  (5 min) detects committed lineups with availability changes.
  Urgent (<3h to practice): immediate email. Non-urgent: 6-hour
  digest. One email per coach, grouped by team with per-team
  sections. Subject includes team names. `opt_in_stale_alerts` on
  `app_user`, `stale_digest_log` table, `EmailType::StaleAlerts`
  unsubscribe, "Lineup change alerts" on email preferences.
- **CI/CD** — GitHub Actions with 4 jobs: lint (fmt + clippy -D
  warnings + tailwind CSS freshness), unit/integration tests
  (workspace minus e2e), e2e tests (Xvfb + WebKitWebDriver via
  nix), Docker image build (nix). All jobs use `nix develop`.
  Deployment pipelines live in separate infra repo.
- **Email send result modal** — reminder/lineup send handlers now
  return a modal (appended to body) instead of replacing tab content
  with a green banner. Shows per-recipient status (Sent / Failed).
  Billing gate shows a lock icon + upgrade message in a modal.
- **Tabbed lineup editor** — tabs are first-class: each is an
  independent lineup the coach can generate into, manually edit, and
  compare. Whichever tab is selected is what gets committed / saved
  as draft. "+ New" button adds tabs, "×" removes (can't delete last).
- **Stripe payment integration** — `stripe_webhook` handler,
  `billing` handler, checkout/portal flow. `stripe_customer_id` on
  tenant. Subscription lifecycle via webhooks.
- **Pricing on landing page** — pricing section on the public landing
  page.
- **Squash migrations for 1.0** — replaced 37 tenant DB + 9 master DB
  incremental migrations with a single `2026-04-26-000000_initial`
  each. Default team seed preserved. Schema.rs files unchanged.
- **E2e test fixes** — repaired 7 broken e2e tests: form vs JSON
  content type, stale `tr[]` → `div[]` selectors, XPath → CSS
  locator for Generate button, added `#tab-bar` to streaming page
  for SSE alternative tabs.

## Open work

### Quick wins

### Architecture / platform

#### Zone reward × side-preference scaling — shipped

Zone reward (S3) is now discounted when the seat is on the rower's
wrong side, proportional to `side_strength` (12% per level:
strength 1→88%, 2→76%, 3→64%, 4→52%, 5→40%). Applied in both the
CP solver and SA post-processor. `Either` rowers are unaffected.

### Long-term / parked

#### Per-team roles

Parked — the role hierarchy (PD > Coach > Member) means a PD
already has full member access on every team. Per-team roles would
only matter if someone needed to be *restricted* on a specific
team, which isn't a real scenario for rowing clubs. Revisit only
if users request it.

#### Regatta lineup generator

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

#### HTMX 4.0 SSE migration

When HTMX 4.0 stabilizes (expected mid-2026), migrate the streaming
alternatives from `htmx-ext-sse` to native fetch streaming. HTMX 4.0
uses `fetch()` + `ReadableStream` natively — no extension needed.
The SSE endpoint stays the same; the frontend drops `hx-ext="sse"`
and `sse-connect`/`sse-swap` in favor of native HTMX streaming
attributes. Also enables POST-based SSE (currently requires GET).
This would let us remove `htmx-ext-sse.js` from the public assets.

#### Discord integration

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

#### Incremental solve animation

Stream intermediate solver results to the client during the primary
solve so the coach sees rowers swapping in real-time while the
spinner runs.

**Design:**

- `ProgressTracker` callback already fires on each improving
  solution. Decode each into placements, diff against previous,
  push swap events through the existing SSE channel.
- New `SolveStreamEvent::Incremental { swaps }` variant. First
  event = full lineup (existing behavior). Intermediates = list
  of `(rower_id, from_boat_seat, to_boat_seat)` swaps. Final =
  full lineup (existing behavior).
- Throttle: stream at most once per 200ms. Buffer the latest
  solution (not diffs) and when the window expires, compute a
  single diff from the last-streamed solution to the latest
  buffered solution. This avoids flooding during early burst
  phase (7 improvements in 63ms is typical) without losing
  state — the client always gets a correct diff to apply.
- Client: animate simple swaps (≤4 placement changes) with CSS
  transitions on row elements. Larger reorganizations crossfade.
  Editor stays non-interactive (spinner + disabled) during solve.
- Don't animate alternatives — only the primary solve.

**Touches:** `lib.rs` (ProgressTracker decode + throttle),
`handlers/solve/stream.rs` (SSE event), `templates/solve/editor.rs`
(JS animation handler).

#### Simulated annealing post-processor for lineup optimization — shipped

SA post-processor in `crates/solver/src/anneal.rs`. Runs 10k
iterations after the CP solve, exploring cross-boat swaps,
within-boat swaps, and bench swaps. Standalone objective evaluator
mirrors all S1–S21 soft constraints. `sa_postprocess: bool` on
`SolveRequest` (default true). Applied to primary in both `solve()`
and `solve_streaming()`. Alternatives not yet SA-processed.

Future tuning: adaptive initial temperature, SA on alternatives,
seeded RNG for deterministic benchmarks.

#### #48 — Deeper unsat diagnostics (relaxation pass / Pumpkin unsat core)

Pre-solve diagnostics shipped. The deeper relaxation-pass work
(re-solve with each hard constraint disabled to identify the
culprit) remains parked pending a Pumpkin API dive.

## Suggested next moves

Nothing urgent — all remaining items are parked/long-term.
