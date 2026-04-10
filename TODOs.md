# lineup_generator — open TODOs

A snapshot of the work backlog as of 2026-04-09. Captures the design
intent behind each pending task so the next session (human or agent)
can resume without re-deriving context. Numbers match the IDs Claude
Code's task system used while building the MVP.

These are listed in roughly the order they should be tackled within
each group, but the groupings themselves are independent — pick the
group that matches your appetite.

## Already shipped

The MVP plus several follow-ups landed in commits `0e740b6` through
`f0ce13c`:

- **#45** Sync sheet UI (`POST /sync` form + result panel)
- **#46** Solver knob form on `/solve/{date}` (`SolveKnobs` query
  string + hidden inputs on commit form)
- **#47** Stop double-solving on commit — handled together with #46
  by passing the same knobs through to the commit handler
- **#49** Inline rower edits on `/rowers` (HTMX outerHTML swap with
  the editable `<tr>` variant)
- **#50** Per-rower detail page at `/rowers/{id}` with inline pair +
  seat affinity CRUD
- **#57** Solver latency measurement → dropped `DEFAULT_BUDGET_SECS`
  from 3 → 1 (wall-time was secretly `budget × top_n` because each
  alternative is a fresh tabu re-solve)
- **#59** Global semaphore bounding concurrent solver runs
  (`SOLVE_CONCURRENCY` env var, defaults to
  `std::thread::available_parallelism()`)

## Open work

### Foundational (unblocks coach features)

#### #63 — Solver-side seat locks: pre-pin (rower, boat, seat) assignments

Coach use case: "I want Alice in stroke of Persephone, no matter
what — solve the rest around that." Today there's no way to express
this; the closest is filtering boats to only Persephone, which
forces *all* of Persephone's seats onto a single re-solve and
doesn't constrain Alice's seat within it.

**Solver work.** Add a `locks: Vec<SeatLock>` field on `SolveRequest`
where each lock is `(RowerId, BoatId, seat_position)`. In the model
build phase (`lineup_solver/src/model.rs`), for each lock:

1. Find the corresponding `x[r, b, s]` decision variable.
2. Post a hard constraint `x[r, b, s] == 1`.
3. Reject the request with an `Unsatisfiable` explanation if the
   lock conflicts with the eligibility filter (wrong side, can't
   cox, etc.) — surface as a `SolveStatus::Unsatisfiable` with a
   structured reason rather than a vague failure.

That's the easy part. The hard parts:

- **Validation:** locks must be self-consistent (no two locks on the
  same seat, no rower locked into two seats, locked rower must be
  available + eligible for that seat). Validate before model build,
  fail fast with an explanatory error.
- **Side eligibility:** a lock for rower R at seat S of boat B
  requires R's side (or `Either`) to match `B.seat_side(S)`. The
  eligibility filter would already drop the x var; detect it
  *before* posting the constraint and return a friendlier error.
- **Cox locks:** seat 0 locks need `is_designated_cox` or `can_cox`
  checks.

**Storage.** Where do locks live in the database?

1. **Per-practice locks** (`practice_lock` table: practice_id,
   rower_id, boat_id, seat_position). Coach sets locks for a
   specific Tuesday. Cleared at commit time or persisted as part of
   the committed lineup.
2. **Per-rower standing locks** (rare — "Alice always strokes
   Persephone"). Less useful in practice.
3. **Ephemeral / request-scoped:** locks are just an input to the
   `SolveRequest`, never stored. Coach sets them via UI form fields,
   they exist for the current solve only. Simpler but loses them on
   a re-solve.

Recommendation: start with per-practice locks (option 1). Schema is
cheap and it's the natural data model for the no-show workflow
(#61), which wants to express "given these fixed assignments, solve
the rest".

**UI.**
- On `/solve/{date}`, each rendered seat in a boat card gets a "lock"
  icon. Click → toggles a lock for that (rower, boat, seat) and
  re-solves with the lock applied.
- On `/rowers/{id}`, optional list of standing locks (skip for v1).
- Locks should be visually distinct in the rendered lineup (e.g. a
  lock icon next to the rower's name, slightly different background
  color).

**Unblocks:** #61 (no-show via lock-everyone-else). #62 (manual swap)
is *not* blocked by this — they're orthogonal (#62 is a direct edit
to existing data, #63 affects future solves).

#### #64 — Boat CRUD: list / add / edit / relinquish

Today boats only enter the database via `fixture::seed_if_empty` —
there's no UI to add, edit, or retire them, and no `GET /boats`
route. Coaches who acquire a new boat or want to flag a damaged one
out of service have no path through the app.

**Direct parallel:** `boat_tracking/src/handlers/boats.rs` has the
full canonical CRUD pattern. Worth reading top-to-bottom before
starting — the `BoatFormInput` / `BoatFormData` / `BoatFormErrors` /
`BoatFormMode` quartet, the per-field validation, the `HX-Push-Url`
+ content+toast OOB swap response shape — everything we want is
already proven there. Mirror it, with two adjustments: (1) drop the
OOB toast bits (we don't have a toast system in `lineup_server`
yet), and (2) use the `db.with_conn` pattern instead of
`pool.get().interact()`.

**DB layer (`crates/db/src/boat/queries.rs`).** Today there's
`Boat::list_sweep` (in-service sweep boats) but probably no public
`list_all`, `get`, `update`. Audit and add what's missing:

- `Boat::list_all(conn)` — every row, ordered by name. Used by the
  admin list view.
- `Boat::get(conn, id) -> Option<Boat>` — mirror `Rower::get`.
  Returns `Option` so the server stays diesel-free.
- `Boat::insert(conn, NewBoat) -> Boat` — likely already exists from
  fixture seeding. Verify.
- `Boat::save(conn, &Boat)` — load + mutate + save pattern. `Boat`
  already derives `AsChangeset`, so the body matches `Rower::save`
  line for line.
- `Boat::relinquish(conn, id, date)` — convenience for soft-delete by
  setting `relinquished_at`. (Or just expose this through `save()` —
  the form can write the field directly.)

**Server (`crates/server/src/handlers/boats.rs` — new).** Routes:

- `GET /boats` — list (in-service + relinquished sections)
- `GET /boats/new` — empty form page
- `POST /boats` — create + redirect to `/boats`
- `GET /boats/{id}` — detail / edit form
- `POST /boats/{id}` — update + redirect

Validation: name non-empty; `weight_class` required; `boat_type` →
`(seat_count, has_cox, oars_per_seat)` via the existing
`BoatType::into_values()` helper; dates parse via
`chrono::NaiveDate`.

Probably skip inline editing (boats have ~9 fields including 3
dates and a rig direction — too much to squeeze into one row). Use
separate `/boats/new` and `/boats/{id}` pages with full forms
instead, like `boat_tracking` does.

**Templates.**
- `templates/boats/list.rs` — table with name, type, weight class,
  status (in service / relinquished YYYY-MM-DD), edit link
- `templates/boats/form.rs` — shared add+edit form, `BoatFormMode::New
  | Edit(BoatId)` discriminator (mirror `boat_tracking::form`)
- Navbar gets a "Boats" link

**Notes.**
- `BoatType` maps to `(seat_count, has_cox, oars_per_seat)` via the
  existing `into_values` helper in `lineup_db::boat::types` — reuse
  it, don't reimplement.
- We only seat sweep boats today, but the schema permits sculling
  boats (`oars_per_seat=2`). The form should expose the boat type
  but the lineup solver will quietly ignore non-sweep boats
  (`Boat::is_sweep`). Worth a note in the form's help text.
- `relinquished_at` is the soft-delete signal — `Boat::in_service()`
  returns false if it's set. The form should let the coach set/clear
  it.
- `stroke_side` is a required column (`Port` or `Starboard`, never
  `Either` — the SQL CHECK enforces this). The form needs a select
  for it. `boat_tracking` doesn't model rig direction the same way —
  we have to add that field ourselves rather than copying it.

Roughly the same shape as #49 (inline rower edits) in scope, but
slightly bigger because the form is more complex and we want
separate pages instead of inline rows.

### Coach features

#### #61 — No-show handling: re-solve with minimal disruption

Coach use case: a rower fails to show up at the dock after a lineup
has been committed. Need a way to mark them as no-show and
regenerate the lineup with the *least possible disruption* to the
rowers who DID show up.

**UI.**
- On `/history/{date}` (the committed lineup view) add a "no-show"
  toggle next to each rower's name.
- Marking a rower no-show should set their availability for the date
  to something like a new `AvailabilityStatus::NoShow` variant (or
  just override the existing status to `No`), then offer a "Re-solve"
  button that runs the solver on the updated snapshot.
- After re-solve, show a diff against the previously committed
  lineup (which seats moved, who's now benched) so the coach can see
  the impact before committing the new version.

**Solver / disruption minimization.** The straightforward path:
just re-run `solve()` on the new snapshot. But that gives a fresh
solution that may shuffle every rower around — not what we want.

Two viable approaches to "minimal disruption":

1. **Lock all unaffected rowers** (depends on #63 — solver-side seat
   locks). For a no-show in seat X of boat Y, lock every other
   (rower, boat, seat) from the previous lineup and only open up the
   affected seats. Trivial extension of #63 once that lands.
   Probably the cleanest approach.
2. **Reward similarity to baseline** as a soft constraint. Add an
   "S14 baseline similarity" objective term that rewards each
   (rower, boat, seat) assignment that matches the previous lineup.
   Symmetric inverse of S7 novelty: where S7 *penalises* matches
   with historical lineups, S14 *rewards* matches with the baseline
   lineup the coach is regenerating from. Solver work; needs a new
   field on `SolveRequest` carrying the baseline.

Approach 1 is simpler and probably sufficient. Approach 2 is more
flexible (allows the solver to make small improvements when locking
would block them). Start with 1.

**Depends on:** #63 (seat locks) for approach 1; nothing for
approach 2.

#### #62 — Manual rower swap in lineups

Coach use case: the solver's proposed lineup is *almost* right, but
the coach wants to swap two specific rowers' seats (e.g. "put Mika
in stroke instead of Lena, move Lena to 7"). Today the only options
are commit-as-is or re-solve with knobs — neither lets you make a
single targeted edit.

**Mental model:** this is a pure UI/data feature, not a solver
feature. The solver proposes a lineup; the coach optionally edits
it; the commit operation saves whatever's currently displayed. No
re-solving on a swap. Both pre-commit (proposed) and post-commit
(saved) edits should work — they're the same operation against
different storage backings.

**UI.**
- Each rendered seat in a boat card gets a click-to-select affordance
  (or drag handle). Click rower A → highlight → click rower B → swap
  their seats, swap their (boat, seat) assignments. Works across
  boats, not just within one boat.
- Special-case the bench / unplaced lists too: a coach should be
  able to swap a fielded rower with a benched one (i.e. a no-show
  workaround that doesn't need #61). Cox seats are independent (can
  only swap with another cox-eligible rower).
- Pure HTMX or alpine `x-data` state, no JS framework.
- Validation hooks (warnings, not blockers): swapping a rower onto
  the wrong side / into a cox seat they can't cox / etc. should show
  a yellow banner but still allow the swap. The coach overrides
  because they have context the solver doesn't.

**State / persistence.** The proposed lineup currently lives only in
the request handler's stack frame. To support edits between solve
and commit, we need somewhere for the in-flight lineup to live.
Three options:

1. **Hidden form fields:** every (boat, seat, rower) tuple
   round-trips through the page as `<input type="hidden">`. Each
   swap POSTs the full current state, server mutates and re-renders.
   Stateless server, no schema. Can get verbose with 10+ boat cards
   on screen but htmx `hx-include` makes it manageable.
2. **Draft table:** new `lineup_draft` rows keyed by
   `(practice_date, ?session_id)`. Each swap is a small POST that
   mutates the draft row, response renders the affected boat cards.
   Stateful server, simpler URLs, but adds schema + a draft
   lifecycle question (when do drafts expire? what happens on a
   re-solve?).
3. **Always commit first, edit committed:** skip the pre-commit
   editing problem entirely by making "commit" cheap (save proposal
   as-is, no friction) and then allowing edits on the committed
   lineup. Schema is unchanged — edits mutate `lineup_seat` rows
   directly via `Lineup::commit_for_boat` again. Workflow shift but
   elegant.

Recommendation: **option 3 first** — it gets the feature shipped
against the existing schema with minimal new code. Coach workflow
becomes: solve → commit → optionally edit. The "commit" verb stops
feeling final and starts feeling like "save draft". If coaches want
pre-commit editing back later, layer on option 1 or 2 then.

Either way the underlying mutation is the same: a "swap two seat
assignments" operation that takes either two `(boat_id, seat)` pairs
or two rower ids and updates the storage backing.

**Out of scope** (kept distinct from #63): no solver involvement, no
re-solving, no locks. A swap is a direct edit. Locks affect *future*
solves; manual swap edits an *existing* lineup.

**No dependency on #63.**

### Quick wins

#### #51 — Practice notes editor

`practice.notes` already exists in the schema. Add a notes textarea
to the solve view (and history view) that POSTs to a new endpoint
and updates `Practice` via `Practice::upsert_by_date(date,
Some(notes))`. Render existing notes on `/history/{date}`.

#### #52 — Proper `Practice::list_committed` query

`handlers/history.rs::list_handler` currently calls
`Lineup::recent_placements(conn, i64::MAX)` and dedupes — wasteful
at scale and load-bearing on a query that wasn't designed for it.
Add a real `Practice::list_with_lineups` (or similar) in `lineup_db`
that returns just the dates (and maybe boat counts) of practices
with committed lineups, newest first.

### Polish

#### #53 — Alternative ranking diff highlighting

In `templates/solve.rs::alternative_block`, highlight which seats /
rowers differ from the primary so the coach can see at a glance what
trade-off each alternative represents. Probably means computing a
diff in the handler and passing per-seat "changed" flags into the
template.

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
`boat_tracking::build_router` does (multi-path resolution). Decide
whether to ship assets next to the binary or via a packaged tarball.

#### #56 — Custom Tailwind build pipeline

`templates/layout.rs` currently pulls Tailwind from
`cdn.tailwindcss.com` — fine for dev, broken offline and bloated in
prod. Add a `tailwind.config.js` that scans
`crates/server/src/**/*.rs` for class names, and a build step that
emits `crates/server/public/tailwind.css`. Mirror `boat_tracking`'s
setup if it has one.

#### #58 — Auth / multi-user

Out of MVP scope but on the future-iterations list. Once a club
wants to deploy this for real, gate writes (commit, sync-sheet,
edits) behind a login. `axum-extra` cookie key infra is already
pulled in for state. Decide whether to roll our own session table or
wire OAuth.

#### #60 — Solver: dedicated thread pool (isolate from blocking pool)

Follow-up to #59. The semaphore now bounds *concurrent* solver
runs, but each run still lands on tokio's shared blocking pool via
`spawn_blocking`. That pool is the same one `deadpool-diesel`'s
`interact()` uses, so a long solve still ties up a blocking-pool
slot that DB calls would otherwise use.

Move `solve()` onto a dedicated non-tokio thread pool so solver CPU
time is isolated from the blocking pool entirely. Two implementation
options:

1. **`rayon::ThreadPool`** — sized to N_cpus (or `solve_concurrency`
   from `AppState`). Submit solves via `pool.install(|| solve(...))`
   from within `spawn_blocking`, OR build a tokio-aware adapter
   (channel + worker threads + oneshot reply).
2. **Custom worker pool** — `std::thread` workers fed by a crossbeam
   channel of `(SolveRequest, oneshot::Sender<SolveResult>)`. The
   `spawn_blocking` call becomes a `channel.send().await;
   receiver.await`. More code, simpler dependencies.

Either way the semaphore from #59 stays — it bounds queue depth into
the dedicated pool. Sized to N_cpus or N_cpus−1 to leave headroom
for the async runtime + blocking pool.

Lower priority than #59 since the semaphore alone gives us correct
backpressure; this is purely an isolation hardening for production
deployments. Worth doing before the multi-tenant work in #58.

### Parked

#### #48 — Unsat / timeout diagnostics — solver + UI

Two-part fix:

1. **Solver work first.** Today `SolveResult { status: Unsatisfiable,
   primary: empty, alternatives: empty }` carries no diagnostic
   detail. Extend the solver to surface *why* a model is infeasible
   — at minimum which hard constraint(s) (H1 boat-completion, H2
   single-seating, H3 side eligibility, H4 cox capability, H5 weight
   class, etc.) contributed. Possible approaches:
   - Pumpkin's core API exposes a conflict explanation; thread it
     through `SolveResult` as a structured `UnsatExplanation` enum.
   - Or, on `Unsatisfiable`, run a lightweight relaxation pass:
     re-solve with each hard constraint individually disabled and
     report which removal made the model satisfiable. Cheap on small
     fleets, expensive at scale — gate behind a flag if needed.
   - Same idea applies to `Timeout`: surface partial bounds /
     best-found-so-far rather than dropping the work entirely.
2. **UI component.** Once the solver carries diagnostics, replace the
   one-liner `status_banner` in `templates/solve.rs` with a richer
   error component that explains the failure (e.g. "no eligible
   cox", "weight class infeasible") and offers next steps — relax
   partial fill, expand boat list, check rower availability. Live in
   `templates/components/` when that module gets created.

The solver work is the prerequisite; the UI is downstream of it.
Parked because the solver-side work needs a deeper Pumpkin API dive.

## Suggested next moves

If picking up from a fresh session:

1. **#52** as a 30-line warm-up that removes a real piece of tech
   debt (`recent_placements(i64::MAX)`).
2. **#63** seat locks — foundational, unblocks #61, lets the coach
   express requests the solver can't infer.
3. **#61** no-show handling — lands easily on top of #63.
4. **#64** boat CRUD — independent of everything else, mirrors the
   boat_tracking pattern closely.
5. **#62** manual swap — start with the "always commit first, edit
   committed" recommendation; cheapest path against the existing
   schema.

Then productionization (#55, #56, #58, #60) and polish (#53, #54)
once the feature surface is closer to complete.
