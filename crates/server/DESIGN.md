# lineup_server design

The coach-facing web UI for the lineup generator. A thin axum server
that wraps `lineup_solver::solve` and `lineup_db` in a server-rendered
HTMX + Maud frontend.

This document is the design reference for the server component. It
captures the architecture, conventions, MVP scope, and future
iterations so a fresh session can pick up the work without
re-exploring the sibling `boat_tracking` project we're modeling on.

## Reference: the boat_tracking sibling project

The server is patterned after `/home/hzimmerman/code/boat_tracking`,
a sibling rowing-club tool that uses the same stack. **Read its source
when in doubt about a convention** — it's the canonical example for
how axum + maud + htmx + alpine fit together in this codebase's
ecosystem.

Key files to mirror from boat_tracking:

| boat_tracking file | What to copy / adapt |
|---|---|
| `Cargo.toml` | Dependency set (axum 0.8, maud 0.26, tower-http, axum-htmx, axum-extra) |
| `src/main.rs` | Tokio entry, env vars, address binding. **Drop the Tauri branches** — we're web-only. |
| `src/lib.rs` | `build_router()` shape — composes handlers + static fallback + state |
| `src/db/state.rs` | `AppState` pattern (clone-friendly wrapper around the pool) |
| `src/handlers/mod.rs` | `PaginationParams`, `maybe_page()` helper, `create_router()` |
| `src/handlers/boats.rs` | Per-handler structure: extract state, query DB, render template |
| `src/templates/layout.rs` | Base `page()` template, navbar with `hx-get` links |
| `src/templates/boats/list.rs` | Pure-function template style (`fn xxx_content(&data) -> Markup`) |
| `public/htmx.min.js`, `public/alpine.min.js` | Static JS assets to copy verbatim |

## Stack and conventions

- **axum 0.8** as the HTTP framework. Routes registered via `Router::new().route(...)`.
- **maud 0.26** for server-rendered HTML templates. Templates are pure
  functions that take borrowed data and return `Markup`. Test-friendly,
  no template-engine state machines.
- **HTMX 2.x** for in-page swaps. Every navbar link uses `hx-get` with
  `hx-target="#content"` and `hx-push-url="true"`. The `id="content"`
  div lives in `templates/layout.rs::page()`.
- **Alpine.js** for tiny client-side reactivity (toggle blocks, modal
  open/close, form-input live preview). Loaded via CDN.
- **Tailwind CSS via CDN** for v1. Add a build pipeline later if we
  want to ship a custom theme. CDN URL goes in
  `templates/layout.rs::page()` `<head>`.
- **tower-http** for the static-asset fallback (`ServeDir::new("public")`)
  and the request-trace layer.
- **axum-htmx** for the `HxRequest` extractor — the `maybe_page()`
  helper uses it to decide between full-page and content-only
  responses.
- **No Tauri.** boat_tracking dual-targets a desktop wrapper; lineup_server
  is web-only. Strip the `[features]` section and conditional `main`
  branches.
- **No charts.** boat_tracking pulls in `plotters`; we don't need it.

## Crate layout

```
crates/server/
├── Cargo.toml
├── DESIGN.md           # this file
├── public/
│   ├── htmx.min.js     # copied from boat_tracking
│   ├── alpine.min.js   # copied from boat_tracking
│   └── favicon.ico     # optional
└── src/
    ├── main.rs         # tokio entry, env vars, build_router(), bind, serve
    ├── lib.rs          # pub fn build_router(conn_string: &str) -> Router
    ├── state.rs        # AppState wrapping lineup_db::Db
    ├── handlers/
    │   ├── mod.rs      # maybe_page() helper, create_router()
    │   ├── practices.rs # GET / and GET /practices — date list
    │   ├── solve.rs    # GET /solve/:date, POST /commit/:date
    │   ├── rowers.rs   # GET /rowers — roster view
    │   └── sync.rs     # POST /sync-sheet — admin trigger
    └── templates/
        ├── mod.rs
        ├── layout.rs   # page() base + navbar()
        ├── practices/
        │   ├── mod.rs
        │   └── list.rs # date dashboard
        ├── solve/
        │   ├── mod.rs
        │   ├── view.rs # full solve view (boat cards + bench + alternatives)
        │   └── boat_card.rs # one boat with seat → rower table
        ├── rowers/
        │   ├── mod.rs
        │   └── list.rs
        └── components/
            ├── mod.rs
            └── common.rs # shared bits: empty state, error banners, etc.
```

## Integration with existing crates

### lineup_db

We already have `lineup_db::Db` (in `crates/db/src/state.rs`) — a
thin wrapper around `deadpool_diesel::sqlite::Pool` that exposes:

```rust
impl Db {
    pub fn connect(conn_str: &str) -> anyhow::Result<Self>;
    pub async fn with_conn<T, F>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error> + Send + 'static,
        T: Send + 'static;
}
```

`Db` is `Clone` and runs migrations on `connect()`. Use it as the
backing for `AppState` rather than building a fresh deadpool from
scratch:

```rust
#[derive(Clone)]
pub struct AppState {
    pub db: lineup_db::Db,
}

impl AppState {
    pub fn new(conn_str: &str) -> anyhow::Result<Self> {
        Ok(Self { db: lineup_db::Db::connect(conn_str)? })
    }
}
```

Handlers use `state.db.with_conn(|conn| ...)` instead of the raw
`pool.get().await.interact(...)` chain that boat_tracking uses. This
matches what the CLI does today (see `crates/cli/src/main.rs`).

### lineup_solver

The server calls `solve()` directly — no IPC, no separate process.
Key types the server needs:

```rust
// crates/solver/src/lib.rs
pub fn solve(snapshot: &DbSnapshot, request: &SolveRequest) -> Result<SolveResult>;

pub struct SolveRequest {
    pub date: NaiveDate,
    pub boats: Vec<BoatId>,           // empty = "all in-service sweep boats"
    pub partial_fill: PartialFillPolicy,
    pub novelty_factor: i32,
    pub time_budget: Option<Duration>,
    pub config: SolverConfig,
    pub top_n: usize,                 // 1 = primary only
    pub tabu_min_diff: i32,           // 2 = "swap one rower" minimum
}

pub struct SolveResult {
    pub status: SolveStatus,          // Satisfied / Unsatisfiable / Timeout
    pub primary: ProposedSolution,
    pub alternatives: Vec<ProposedSolution>,
}

pub struct ProposedSolution {
    pub lineups: Vec<ProposedLineup>, // one per candidate boat, used = true if fielded
    pub unplaced: UnplacedRowers,
}

pub struct ProposedLineup {
    pub boat_id: BoatId,
    pub boat_name: String,
    pub used: bool,
    pub seats: Vec<(i32, RowerId)>,   // (seat_position, rower_id), seat 0 = cox
}

pub struct UnplacedRowers {
    pub to_sculling: Vec<RowerId>,    // can_scull = true, redirect to scullers
    pub benched: Vec<RowerId>,        // can_scull = false, dock today
}
```

**Solve invocation policy.** v1 calls `solve()` synchronously inside
the request handler. The solver respects `time_budget`, so the
worst-case latency is bounded. Default UI budget: **3 seconds**
(snappier than the CLI's 10s default — coaches expect interactive
responsiveness).

If the 3s budget proves too tight on real fleets, switch to
`tokio::task::spawn_blocking` and a job queue. Not v1.

### lineup_sheets

`POST /sync-sheet` calls `lineup_sheets::sync_csv` after fetching
the spreadsheet via `reqwest`. The CLI's `cmd_sync_sheet` in
`crates/cli/src/main.rs` is a working reference — port the same
fetch + `db.with_conn` pattern into a handler.

## Routes (MVP)

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/` | Redirect to `/practices` |
| `GET` | `/practices` | Dashboard: list of upcoming practice dates with availability counts |
| `GET` | `/solve/:date` | Solve view for the given date (date format `YYYY-MM-DD`) |
| `POST` | `/commit/:date` | Persist the primary lineup, redirect to `/history/:date` |
| `GET` | `/history` | List of committed practices |
| `GET` | `/history/:date` | Detail view of a committed lineup |
| `GET` | `/rowers` | Roster view (read-only in v1) |
| `POST` | `/sync-sheet` | Trigger Google Sheet sync (form takes spreadsheet ID + gid) |
| `*` (fallback) | `/public/*` | Static asset serving via `tower_http::services::ServeDir` |

**Out of v1 scope:**
- Auth / multi-user
- Inline editing of rower attributes / affinities
- Real-time updates (refresh-to-update is fine)
- Mobile-first responsive design (desktop-targeted, but mobile-tolerant via Tailwind defaults)
- Top-N alternatives in commit flow (only primary persists)

## Solve view (`GET /solve/:date`)

The flagship page. Layout:

```
┌──────────────────────────────────────────────────────────────────┐
│  Navbar: Practices | Solve | History | Rowers | Sync             │
├──────────────────────────────────────────────────────────────────┤
│  Header: 2026-04-11 · 11 rowers available · 3 candidate boats    │
│  ┌──────────────┬──────────────────────────────────────────┐    │
│  │ Solver knobs │ Primary lineup                           │    │
│  │              │ ┌──────────────────────────────────────┐ │    │
│  │ ☐ partial=2  │ │ Persephone (8+)                      │ │    │
│  │ ☐ novelty=1  │ │  cox  Mika                           │ │    │
│  │ alts: [3]    │ │  s1   Alice  [Medium/Expert/Strong]  │ │    │
│  │              │ │  s2   Erin   [...]                   │ │    │
│  │ [ Re-solve ] │ │  ...                                 │ │    │
│  │              │ └──────────────────────────────────────┘ │    │
│  │ [ Commit  ]  │                                          │    │
│  │              │ Benched: Ivan, Lena                      │    │
│  │              │ To sculling: (none)                      │    │
│  │              │                                          │    │
│  │              │ ▶ Show 2 alternatives                    │    │
│  │              │   ┌───────────────────────────────┐      │    │
│  │              │   │ Alternative #2: Persephone    │      │    │
│  │              │   │  cox  Lena                    │      │    │
│  │              │   │  ...                          │      │    │
│  │              │   │ Benched: Ivan, Mika           │      │    │
│  │              │   └───────────────────────────────┘      │    │
│  └──────────────┴──────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
```

The "Re-solve" button is a `<form>` POST or htmx `hx-post` that
re-runs `solve` with the chosen knobs and swaps `#content`. The
"Commit" button POSTs to `/commit/:date` and redirects to history.

**Boat card template** (`templates/solve/boat_card.rs`):
- Takes `&ProposedLineup` and a name lookup closure (RowerId → name + traits)
- Renders the boat name, fielded/skipped status, and per-seat rows
- Cox row labeled `cox`, rowing seats labeled `s1` through `s8`
- Each row shows the rower name with their `[weight/skill/strength]` and side as a small subtitle

**Alternatives toggle** (Alpine.js):
```html
<div x-data="{ open: false }">
  <button @click="open = !open">▶ Show alternatives</button>
  <div x-show="open">...</div>
</div>
```

No HTMX swap needed — alternatives are rendered server-side as part
of the initial solve response, just hidden by default.

## AppState / handler patterns

```rust
// state.rs
#[derive(Clone)]
pub struct AppState {
    pub db: lineup_db::Db,
}

impl AppState {
    pub fn new(conn_str: &str) -> anyhow::Result<Self> {
        Ok(Self { db: lineup_db::Db::connect(conn_str)? })
    }
}

// handlers/practices.rs
use axum::{extract::State, response::Html, http::StatusCode};
use axum_htmx::HxRequest;

pub async fn practices_list_handler(
    State(state): State<AppState>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let practices = state.db
        .with_conn(|conn| Practice::list_upcoming(conn))
        .await
        .map_err(internal_error)?;
    let content = templates::practices::list_content(&practices);
    Ok(super::maybe_page("Practices", content, hx))
}

// handlers/mod.rs
pub fn maybe_page(title: &str, content: Markup, HxRequest(is_htmx): HxRequest) -> Html<String> {
    if is_htmx {
        Html(content.into_string())
    } else {
        Html(templates::layout::page(title, content).into_string())
    }
}

fn internal_error<E: std::fmt::Debug>(error: E) -> StatusCode {
    tracing::error!(?error, "handler error");
    StatusCode::INTERNAL_SERVER_ERROR
}
```

## main.rs / build_router shape

```rust
// main.rs
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let conn_string = std::env::var("DATABASE_URL").unwrap_or_else(|_| "lineup.sql".to_string());
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);

    let app = lineup_server::build_router(&conn_string)?;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    println!("running at http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

// lib.rs
use axum::{routing::get, Router};
use crate::state::AppState;

pub mod state;
pub mod handlers;
pub mod templates;

pub fn build_router(conn_string: &str) -> anyhow::Result<Router> {
    let state = AppState::new(conn_string)?;
    Ok(Router::new()
        .merge(handlers::create_router())
        .fallback_service(tower_http::services::ServeDir::new("crates/server/public"))
        .with_state(state)
        .layer(tower_http::trace::TraceLayer::new_for_http()))
}
```

(The `crates/server/public` path is dev-friendly; production should
fall back to `exe_dir/public` like boat_tracking does. v1 doesn't need
the multi-path resolution.)

## MVP scope (first focused session)

**Must have:**
1. `Cargo.toml` deps + workspace updates
2. `state.rs` with `AppState` wrapping `lineup_db::Db`
3. `main.rs` + `lib.rs` + `handlers/mod.rs` skeleton with `maybe_page()`
4. `templates/layout.rs` with `page()` and a navbar (Practices / Solve / History / Rowers)
5. `public/` populated with htmx.min.js + alpine.min.js (copy from boat_tracking) + favicon
6. `GET /` redirect → `/practices`
7. `GET /practices` — date list with rower-count summaries (calls `Practice::list_upcoming` + per-date availability count)
8. `GET /solve/:date` — calls `solve()` with default config + 3s budget, renders boat cards + benched/sculling lists; **alternatives shown server-side, toggled with Alpine.js**
9. `POST /commit/:date` — persists the primary via `Lineup::commit_for_boat`, redirects
10. End-to-end smoke test: start server, hit `/`, click through to a solve, eyeball the rendered HTML

**Stretch (same session if time):**
- `GET /history` + `GET /history/:date`
- `GET /rowers` (read-only roster)

**Defer to a follow-up session:**
- `POST /sync-sheet` (need a small form for spreadsheet ID + gid)
- Inline editing of rower attributes
- Solver knob form (partial-fill, novelty, alternatives count) — v1 uses defaults
- Custom Tailwind theme / build pipeline
- Static asset path resolution for production
- Auth

## Future iterations

In rough order of value once the MVP is in:

1. **Solver knob form** on the solve page — partial-fill, novelty, alternatives count, weight overrides
2. **Inline rower edits** — click a name → modal with skill/strength/side/etc. → save → re-render
3. **Inline pair/seat affinity edits** — same pattern
4. **Sync workflow** — admin form for spreadsheet ID + a "preview before sync" diff view
5. **Alternative ranking diff** — for Top-N, highlight which rowers move between primary and each alternative
6. **Print-friendly stylesheet** — coaches print the lineup before going on the water
7. **Practice notes editor** — `practice.notes` is already in the schema
8. **Multi-user / auth** — once a club wants this for real
9. **Pull boat_tracking's status / event-history infra** for tracking solver runs over time

## Open questions for the next session

- **Solve invocation latency**: 3s budget is a guess. Measure on a realistic fixture; tune up or down.
- **HTMX form submission**: should the "Re-solve" button be a real form POST (browser navigation) or `hx-post` (in-page swap)? I lean toward `hx-post` with a loading indicator since the solve takes seconds.
- **Static asset paths**: dev uses `crates/server/public`, production probably uses `exe_dir/public`. Do we need both for v1? Probably not.
- **Error UX**: what does the page look like when `solve()` returns `Unsatisfiable`? Need an error component in `templates/components/`.

---

When the next session starts, the right path is:

1. Read this doc end-to-end.
2. Skim `boat_tracking/src/lib.rs`, `boat_tracking/src/main.rs`,
   `boat_tracking/src/handlers/mod.rs`, `boat_tracking/src/handlers/boats.rs`,
   `boat_tracking/src/templates/layout.rs` to internalize the conventions.
3. Update `crates/server/Cargo.toml` with the new deps.
4. Build out the MVP in the order listed above.
5. Smoke-test end-to-end before committing.
