# Auth & multi-tenancy design

Design reference for adding JWT-based authentication, multi-tenancy
(one SQLite file per rowing club), and team-scoped roles to the
lineup generator. Captures the decisions from the Q&A session so a
future implementation session can proceed without re-deriving context.

## Terminology

- **Tenant**: a rowing club. One SQLite file per tenant.
- **Team**: a named roster + practice schedule within a tenant that
  shares the club's fleet. E.g. "Morning Sweep", "Evening Sweep".
  Rowers and coaches can belong to multiple teams but view one at a
  time.
- **User**: an auth identity (email + password hash). Lives in the
  tenant DB alongside rowing domain data.
- **Rower**: the rowing-domain entity (skill, strength, side, etc.).
  Linked to a user via `rower.user_id` FK — a real FK within one
  SQLite file. A user who doesn't row (e.g. a Program Director) has
  no rower record.
- **Superuser**: the system operator. Credentials live in the master
  DB, not any tenant DB.

## Architecture

### Two-tier database layout

```
master.db                          # global, tiny
├── tenant (id, name, slug, db_path)
└── superuser (id, email, password_hash)

club_alpha.db                      # one per tenant
├── user (id, email, password_hash, name, status, ...)
├── team (id, name, ...)
├── team_membership (team_id, rower_id)
├── team_coach (team_id, user_id)
├── user_role (user_id, role)      # tenant-wide roles
├── rower (id, ..., user_id FK)    # existing + new nullable FK
├── boat (id, ...)                 # shared fleet, unchanged
├── practice (id, team_id, ...)    # gains team_id FK
├── availability (rower_id, team_id, date, status)  # gains team_id
├── lineup / lineup_seat           # inherit team via practice
├── pair_affinity                  # tenant-wide, unchanged
└── rower_seat_affinity            # tenant-wide, unchanged
```

**Why separate files, not row-level isolation:**
- Real FKs between `user` and `rower` within one file. No cross-DB
  consistency issues.
- Each tenant DB can be independently backed up / restored / migrated.
- SQLite's single-writer model is per-file. Separate files mean
  tenants don't contend on writes.
- The master DB is tiny (tenant list + superuser creds) and rarely
  written.

**The only cross-DB operation** is the login email lookup: scan each
tenant DB's `user` table for the email. Realistic deployment is a
handful of clubs, so this is negligible. Cache the email → tenant
mapping in memory if it ever matters.

### Multi-tenant request routing

No subdomains, no path prefixes. The JWT carries `tenant_id`, and the
server resolves which SQLite file to use per request from the token
claims.

Every handler's `State<AppState>` gains a `tenant_db(&self,
tenant_id) -> &Db` method that returns the pooled connection for that
tenant. The current single-`Db` field becomes the master DB; tenant
DBs are lazily opened and cached in a `DashMap<TenantId, Db>` or
similar.

### JWT design

- **Signing**: HS256 with a server-generated secret (env var
  `JWT_SECRET`). Rotate by restarting the server with a new secret;
  all existing tokens invalidate.
- **Claims**: `{ sub: user_id, tenant_id, role, active_team_id, exp, iat }`
- **Transport**: `Set-Cookie: token=<jwt>; HttpOnly; Secure; SameSite=Lax; Path=/`
  for the browser. `Authorization: Bearer <jwt>` accepted as an
  alternative for future API clients.
- **Lifetime**: 24 hours. No refresh tokens for v1; the user logs in
  again when it expires.
- **Team switching**: changing the active team re-issues the JWT with
  a new `active_team_id` claim. The server validates that the user
  is a member/coach of the team they're switching to.

## Roles and permissions

### Role model

| Role             | Scope       | Notes                                          |
|------------------|-------------|-------------------------------------------------|
| Member           | per-team    | Rower/cox on specific teams                    |
| Coach            | per-team    | Can coach multiple teams; views one at a time  |
| Program Director | tenant-wide | Cross-team admin; sees everything              |
| Superuser        | global      | System operator; lives in master DB only       |

A user's role is stored as a tenant-wide enum in `user_role.role`.
Team-level scoping (which teams a member/coach belongs to) is
expressed through `team_membership` and `team_coach` join tables, not
through the role itself.

A single user can be both a Member of team A and a Coach of team B.
The JWT's `role` field carries the highest privilege level; the
team-level join tables determine what that role can see.

### Permission matrix

| Action                      | Member | Coach | Program Dir | Superuser |
|-----------------------------|--------|-------|-------------|-----------|
| View own availability/attrs | ✓      | ✓     | ✓           | ✓         |
| Edit own availability/attrs | ✓      | ✓     | ✓           | ✓         |
| View team lineup            | ✓      | ✓     | ✓           | ✓         |
| Run solver                  |        | ✓     | ✓           | ✓         |
| Commit/edit lineups         |        | ✓     | ✓           | ✓         |
| Edit any rower's attrs      |        | ✓     | ✓           | ✓         |
| Edit affinities             |        | ✓     | ✓           | ✓         |
| Edit boats                  |        |       | ✓           | ✓         |
| Manage team membership      |        |       | ✓           | ✓         |
| Manage coaches/roles        |        |       | ✓           | ✓         |
| Send invite emails          |        |       | ✓           | ✓         |
| Sync sheet                  |        | ✓     | ✓           | ✓         |
| Cross-team schedule view    |        | flag* | ✓           | ✓         |
| Manage tenants              |        |       |             | ✓         |

*\*Coach cross-team visibility controlled by a tenant-level config
flag, set by the Program Director only.*

### Active team context

Every role views one team at a time via a "current team" selector in
the nav. The JWT's `active_team_id` claim determines the team context
for each request. Program Directors additionally access tenant-wide
admin views (fleet management, role management, cross-team schedule
overview) that aren't team-scoped.

## Account lifecycle

### 1. Tenant creation (superuser)

Superuser creates a tenant via CLI or admin endpoint. This:
- Inserts a row in `master.db → tenant`
- Creates a new SQLite file at the configured path
- Runs migrations on the new file
- Creates the first Program Director user in the tenant DB

### 2. Rower import (sheet sync or manual entry)

Program Director imports rowers via sheet sync (existing flow) or
manual entry. This creates `rower` rows in the tenant DB. No `user`
account yet — just domain data.

### 3. Invite (Program Director)

Program Director clicks "send invite" next to a rower (or bulk
invites). This:
- Creates a `user` row in the tenant DB with `status = 'invited'`
- Links `rower.user_id = user.id`
- Generates a one-time invite token (random, stored hashed in a
  `user_invite` table with expiry)
- Emails the rower a link: `/invite/{token}`

### 4. Account activation (rower)

Rower clicks the invite link → sets a password → `user.status`
becomes `'active'` → JWT issued → redirected to their team's
practice view.

### 5. Normal login

Email + password on a tenant-agnostic login page:
1. Server scans tenant DBs for the email (or checks a cached
   email → tenant_id mapping).
2. If found in one tenant → verify password → issue JWT.
3. If found in multiple → present a "which club?" picker → verify
   against the chosen tenant → issue JWT.
4. If not found → generic "invalid credentials" (don't leak whether
   the email exists).

## Schema changes

### Master DB (new file: `master.db`)

```sql
CREATE TABLE tenant (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,          -- URL-safe identifier
    db_path TEXT NOT NULL UNIQUE,       -- filesystem path to tenant SQLite
    created_at DATETIME NOT NULL
);

CREATE TABLE superuser (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at DATETIME NOT NULL
);
```

### Tenant DB additions (migration on existing schema)

```sql
-- Auth identity. Separate from `rower` so non-rowing users
-- (Program Directors who don't row) have accounts too.
CREATE TABLE user (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT,                 -- NULL until invite is accepted
    name TEXT NOT NULL,
    status TEXT CHECK( status IN ('invited','active','disabled') ) NOT NULL DEFAULT 'invited',
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

-- Tenant-wide role. A user has exactly one role per tenant.
-- Team-level scoping is via team_membership / team_coach.
CREATE TABLE user_role (
    user_id INTEGER PRIMARY KEY NOT NULL,
    role TEXT CHECK( role IN ('Member','Coach','ProgramDirector') ) NOT NULL,
    FOREIGN KEY (user_id) REFERENCES user(id) ON DELETE CASCADE
);

-- A named roster + practice schedule that shares the tenant's fleet.
CREATE TABLE team (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    created_at DATETIME NOT NULL
);

-- Rower ↔ team membership. A rower can belong to multiple teams.
CREATE TABLE team_membership (
    team_id INTEGER NOT NULL,
    rower_id INTEGER NOT NULL,
    PRIMARY KEY (team_id, rower_id),
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE,
    FOREIGN KEY (rower_id) REFERENCES rower(id) ON DELETE CASCADE
);

-- Coach ↔ team assignment. A coach can coach multiple teams.
CREATE TABLE team_coach (
    team_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    PRIMARY KEY (team_id, user_id),
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES user(id) ON DELETE CASCADE
);

-- One-time invite tokens for account activation.
CREATE TABLE user_invite (
    token_hash TEXT PRIMARY KEY NOT NULL,  -- bcrypt or sha256 of random token
    user_id INTEGER NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    FOREIGN KEY (user_id) REFERENCES user(id) ON DELETE CASCADE
);

-- Tenant-level configuration flags.
CREATE TABLE tenant_config (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
-- Seed: INSERT INTO tenant_config VALUES ('coach_cross_team_visibility', 'false');

-- Existing table modifications:
ALTER TABLE rower ADD COLUMN user_id INTEGER REFERENCES user(id) ON DELETE SET NULL;
ALTER TABLE practice ADD COLUMN team_id INTEGER REFERENCES team(id);
ALTER TABLE availability ADD COLUMN team_id INTEGER REFERENCES team(id);
```

### Availability key change

Currently `availability` has `PRIMARY KEY (rower_id, date)`. With
per-team availability, the key becomes `(rower_id, team_id, date)`.
This requires dropping and recreating the table (SQLite doesn't
support `ALTER TABLE ... DROP PRIMARY KEY`).

### Practice uniqueness change

Currently `practice.date` is `UNIQUE`. With teams, the constraint
becomes `UNIQUE(team_id, date)` — two teams can practice on the same
date.

## Impact on existing code

### lineup_db

- `DbSnapshot::for_date(conn, date)` → `for_team_date(conn, team_id, date)`.
  Filters rowers by `team_membership`, availability by
  `(team_id, date)`. Boats stay unfiltered (shared fleet).
- `Availability::map_for_date` → `map_for_team_date`.
- `Availability::upcoming_dates` → scoped by team_id.
- `Practice::upsert_by_date` → takes team_id.
- `Practice::list_committed` → scoped by team_id.
- `Lineup::commit_for_boat` → unchanged (practice already carries team via FK).

### lineup_solver

No changes. The solver operates on a `DbSnapshot` which is already
a "point-in-time view of everything needed for one practice". The
team scoping happens at snapshot construction time, not inside the
solver.

### lineup_server

- `AppState` gains `master_db: Db` and `tenant_dbs: DashMap<TenantId, Db>`
  (or equivalent lazy cache). Current `db` field becomes per-tenant.
- Every handler gains a JWT-extraction middleware that resolves
  `(tenant_id, user_id, role, active_team_id)` from the token.
- A `require_role(min_role)` middleware layer gates endpoints by
  permission level.
- New handler modules: `auth.rs` (login, invite, activate),
  `teams.rs` (team CRUD, membership), `admin.rs` (role management,
  tenant config).
- Existing handlers are unchanged in logic but scoped: the `db` they
  operate on comes from `state.tenant_db(claims.tenant_id)` and
  queries are filtered by `claims.active_team_id`.

### lineup_sheets

- `sync_csv` gains a `team_id` parameter so availability rows are
  scoped to the correct team. The rower upsert path stays
  tenant-wide (rowers belong to the tenant, not a team).

## Future: gendered programs

For men's/women's team splits, boats in the program have a strong
affinity (or hard constraint) toward one team. The data model
supports this via a future `boat_team_affinity` table or a nullable
`boat.team_id` FK. The solver would filter `snapshot.sweep_boats`
by team affinity before building the model. Not implemented now;
the current design doesn't preclude it.

## Implementation plan

### Phase 1: Team structure (no auth yet)

Add the `team`, `team_membership`, `team_coach` tables and migrate
`practice` + `availability` to carry `team_id`. Update
`DbSnapshot::for_date` → `for_team_date`. Add a team selector to
the nav. All existing functionality works but is team-scoped.

This can ship without auth — the team selector is just a nav
dropdown, and the server trusts whoever clicks it. Auth gates the
"who can do what" question; team structure gates "what data do they
see."

### Phase 2: Master DB + tenant isolation

Create the master DB schema. Refactor `AppState` from one `Db` to
`master_db + tenant_dbs`. Add a `tenant_id` claim path through the
request pipeline. For now, hard-code a single tenant (the existing
DB) so nothing visibly changes — the plumbing is in place but only
one tenant exists.

### Phase 3: JWT auth

Add the `user`, `user_role`, `user_invite` tables to the tenant
schema. Implement:
- `POST /login` (email + password → JWT cookie)
- `POST /logout` (clear cookie)
- `GET /invite/{token}` + `POST /invite/{token}` (set password)
- `require_auth` middleware (extract + validate JWT on every request)
- `require_role(min)` middleware
- Program Director UI for sending invites

### Phase 4: Multi-tenant

Superuser CLI or admin UI for creating tenants. The login flow gains
the "which club?" picker for multi-tenant emails. Each tenant gets
its own SQLite file, connection pool, and solver pool.

### Phase 5: Rower self-service

Members can log in and:
- View their team's lineup (read-only, no solve/commit controls)
- Edit their own availability (replaces the sheet sync path for
  responsive clubs)
- Edit their own attributes (weight class, side, skill, etc.)
  within guard rails (coach can lock specific fields)

This is the payoff for the auth investment — rowers manage their own
data instead of everything flowing through a spreadsheet.
