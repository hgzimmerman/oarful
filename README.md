# lineup_generator

Constraint-based lineup generator for sweep rowing clubs. A coach-facing
web app that assigns rowers to boats and seats, balancing skill, weight
class, side preference, pair affinities, and a dozen other soft
constraints via a Pumpkin CP solver.

## License

    lineup_generator — lineup management for sweep rowing clubs
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

```bash
# 1. Seed fixture data (creates lineup.sql with rowers, boats, team,
#    availability, and a dev user)
cargo run -p lineup_cli -- solve 2026-04-11

# 2. Start the server (creates master.db for tenant registry)
#    Set JWT_SECRET so tokens survive restarts (avoids re-login).
JWT_SECRET=dev cargo run -p lineup_server
```

Open `http://127.0.0.1:3000` and log in:

- **Email:** `coach@test.com`
- **Password:** `12345`

The dev user is a ProgramDirector with full access.

## Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `lineup.sql` | Path to the tenant SQLite database |
| `MASTER_DB` | `master.db` | Path to the master tenant-registry DB |
| `PORT` | `3000` | HTTP listen port |
| `PUBLIC_DIR` | `crates/server/public` | Static assets directory |
| `JWT_SECRET` | random (dev) | Secret for signing JWT tokens; set in production |
| `SOLVE_CONCURRENCY` | CPU count | Max concurrent solver runs |
| `ORIGIN` | *(none)* | Base URL for full invite links (e.g. `http://localhost:3000`) |

## Project structure

```
crates/
  db/         — diesel models, migrations, fixture seeder
  master_db/  — tenant registry (multi-tenancy)
  solver/     — Pumpkin CP model, soft/hard constraints
  sheets/     — Google Sheets CSV sync
  server/     — axum web server, maud templates, HTMX UI
  cli/        — command-line solve + bench tools
```

## Running tests

```bash
cargo test --workspace
```

To regenerate solver baseline snapshots after a fixture change:

```bash
UPDATE_BASELINES=1 cargo test -p lineup_solver --test baseline
```
