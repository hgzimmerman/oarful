# CLAUDE.md

## Build & Test
- `cargo build -p lineup_server` — fast iteration (server + deps only)
- `cargo test -p lineup_solver` — solver unit + baseline tests
- `UPDATE_BASELINES=1 cargo test -p lineup_solver --test baseline` — regenerate solver snapshots after heuristic changes
- `cargo test --workspace` — full test suite

## Architecture
- Rust workspace: db, master_db, solver, sheets, server, cli
- Server: axum handlers → maud templates → HTMX + Alpine.js + Tailwind CDN
- Solver: Pumpkin CP with soft/hard constraints, greedy fleet pre-selection
- DB: per-tenant SQLite (diesel ORM) + master tenant registry
- Auth: JWT with roles (Member < Coach < ProgramDirector)

## Code Patterns
- Maud templates use `"@click"=` (quoted) for Alpine directives
- Do NOT call `Alpine.initTree()` on HTMX afterSettle — causes double-init breaking editor state. Alpine v3 with `defer` auto-initializes via MutationObserver
- Dynamic elements inside Alpine components must use `addEventListener` not `@click` attributes (Alpine won't bind directives added via JS)
- Maud auto-escapes text in `<script>` tags — use `maud::PreEscaped()` for inline JS containing `&&`, `<`, etc.
- Prefer HTMX server-driven swaps over client-side JS DOM manipulation
- `DetailPermissions::coach()` vs `::member(level)` controls field-level editing
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
