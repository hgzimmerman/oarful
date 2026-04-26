# Oarful

Practice lineup management for rowing clubs. Assigns rowers to boats
and seats, balancing skill, weight class, side preference, pair
affinities, and 20+ other soft constraints via a constraint-programming
solver.

**Stack:** Rust, Axum, HTMX, Alpine.js, maud templates, Pumpkin CP
solver, per-tenant SQLite.

## Features

- Constraint solver generates optimal seat assignments in seconds
- Streaming alternatives via SSE — compare multiple lineups side by side
- Manual editor with drag-and-drop swaps, boat transfers, seat locks
- Availability tracking with coach-editable attendance grid
- Email reminders, lineup notifications, magic-link sign-in
- Multi-tenant with self-service club onboarding and Stripe billing
- Google Sheets sync for roster import
- Audit log, data export/import, demo mode

## License

    Oarful — lineup management for rowing clubs
    Copyright (C) 2026  Henry Zimmerman

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as
    published by the Free Software Foundation, either version 3 of the
    License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

## Quick start

Requires [nix](https://nixos.org/download/) with flakes enabled.

```bash
# Enter the dev shell
nix develop

# Seed fixture data (creates lineup.sql with rowers, boats, team,
# availability, and a dev user)
cargo run -p lineup_cli -- seed

# Start the server (creates master.db for tenant registry)
# Set JWT_SECRET so tokens survive restarts.
JWT_SECRET=dev cargo run -p lineup_server
```

Open `http://127.0.0.1:3000` and log in:

- **Email:** `coach@test.com`
- **Password:** `12345`

The dev user is a ProgramDirector with full access.

## Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MASTER_DB` | `master.db` | Path to the master tenant-registry DB |
| `DATA_DIR` | `data` | Directory for per-tenant SQLite files |
| `PORT` | `3000` | HTTP listen port |
| `HOST` | `127.0.0.1` | Bind address |
| `PUBLIC_DIR` | `crates/server/public` | Static assets directory |
| `JWT_SECRET` | random (dev) | Secret for signing JWT tokens; set in production |
| `ORIGIN` | *(none)* | Base URL for absolute links in emails (e.g. `https://oarful.com`) |
| `SOURCE_URL` | GitHub repo | URL shown in AGPL source link footer |
| `SOLVE_CONCURRENCY` | CPU count | Max concurrent solver runs |
| `STRIPE_SECRET_KEY` | *(none)* | Enables Stripe billing when set |
| `STRIPE_PUBLISHABLE_KEY` | — | Required with `STRIPE_SECRET_KEY` |
| `STRIPE_WEBHOOK_SECRET` | — | Required with `STRIPE_SECRET_KEY` |
| `STRIPE_PRICE_ID` | — | Required with `STRIPE_SECRET_KEY` |
| `RUST_LOG` | `info` | Tracing filter directive |

## Project structure

```
crates/
  db/         — diesel models, migrations, fixture seeder
  master_db/  — tenant registry (multi-tenancy)
  solver/     — Pumpkin CP model, soft/hard constraints, SA post-processor
  sheets/     — Google Sheets CSV sync
  server/     — axum web server, maud templates, HTMX + Alpine.js UI
  cli/        — CLI commands (seed, reset, set-billing)
```

## Running tests

```bash
# Quick pre-commit suite (~3s)
cargo test --workspace --exclude lineup_e2e --exclude lineup_solver \
  && cargo test -p lineup_solver --test constraints

# Full unit/integration tests
cargo test --workspace --exclude lineup_e2e

# E2e tests (needs Xvfb + WebKitWebDriver from nix shell)
cargo test -p lineup_e2e

# Lint
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

## Deployment

The app deploys as a Docker image built via nix. See `Dockerfile` and
`fly.toml` for the Fly.io configuration.

```bash
fly deploy
```

Secrets are set via `fly secrets set`:

```bash
fly secrets set JWT_SECRET=...
fly secrets set STRIPE_SECRET_KEY=... STRIPE_PUBLISHABLE_KEY=... \
  STRIPE_WEBHOOK_SECRET=... STRIPE_PRICE_ID=...
```
