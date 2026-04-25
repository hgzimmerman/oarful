# CLAUDE.md

## Build & Test
- `cargo build -p lineup_server` — fast iteration (server + deps only)
- `cargo test -p lineup_solver` — solver unit + baseline tests
- `UPDATE_BASELINES=1 cargo test -p lineup_solver --test baseline` — regenerate solver snapshots after heuristic changes
- `cargo test --workspace` — full test suite
- `cargo test --workspace --exclude lineup_e2e` — unit/integration tests only (no browser deps)
- `cargo test -p lineup_e2e` — e2e tests (needs Xvfb + WebKitWebDriver from nix shell)
- `cargo clippy --workspace -- -D warnings` — lint (enforced in CI)
- `cargo fmt --all -- --check` — format check (enforced in CI)

## Architecture
- Rust workspace: db, master_db, solver, sheets, server, cli
- Server: axum handlers → maud templates → HTMX + Alpine.js + Tailwind CDN
- Solver: Pumpkin CP with soft/hard constraints, greedy fleet pre-selection
- DB: per-tenant SQLite (diesel ORM) + master tenant registry
- Auth: JWT with roles (Member < Coach < ProgramDirector)
- Background tasks: `tokio::spawn` with `interval()` in `lib.rs` (demo cleanup, sync poll, audit cleanup, stale alerts)
- E2E: `TestInstance` in `e2e/src/lib.rs` — in-process Axum + Xvfb + WebKitWebDriver; `ChannelMailer` captures sent emails

## Code Patterns
- Maud templates use `"@click"=` (quoted) for Alpine directives
- Do NOT call `Alpine.initTree()` on HTMX afterSettle — causes double-init breaking editor state. Alpine v3 with `defer` auto-initializes via MutationObserver
- Dynamic elements inside Alpine components must use `addEventListener` not `@click` attributes (Alpine won't bind directives added via JS)
- Maud auto-escapes text in `<script>` tags — use `maud::PreEscaped()` for inline JS containing `&&`, `<`, etc.
- Prefer HTMX server-driven swaps over client-side JS DOM manipulation
- `DetailPermissions::coach()` vs `::member(bv, mrm)` controls field-level editing + bucket visibility
- Modals: `hx-target="body" hx-swap="beforeend"` to append modal, JS `remove()` on backdrop+modal to close
- Email send handlers return modals (not banners) via `send_result_modal()` / `send_result_billing_gate()`
- Tenant iteration for background tasks: `state.master_db` → list tenants → `state.tenant_db(id)` per tenant, skip demos
- `clippy.toml` sets `too-many-arguments-threshold = 10` (for Mailer trait methods)
- `SolveKnobs` round-trips via URL query params — new fields need `#[serde(default)]`
- Print CSS: `no-print` hides elements, `print-break` avoids page-break inside
- Responsive: grids start `grid-cols-1` and step up at `sm:`/`md:` breakpoints
- Touch targets: minimum `py-2` on interactive elements (44px target)

## Gotchas
- Solver timeout (SolveStatus::Timeout) means zero results; timeout-with-best-result maps to Satisfied
- Greedy fleet selection reserves min_seats (not total) per boat to avoid over-consumption
- `SolveRequest.boats` when empty means "all sweep boats" — must explicitly pass boat IDs to restrict
- Tailwind classes in JS strings (Alpine toggleBoat, addPoolPill) must stay in sync with template classes
- Don't include rower/user names from test data in commit messages
- Demo fixture creates team ID 2 (migration seeds "Default" team as ID 1) — e2e tests must find the right team
- Migrations: prefer `ALTER TABLE ADD COLUMN` over table recreation; old columns can stay (diesel `Selectable` ignores them)
- E2e tests for demo tenants: promote with `UPDATE tenant SET billing_status = 'grandfathered', demo_expires_at = NULL` + `refresh_tenant_configs()` to test features gated by billing
