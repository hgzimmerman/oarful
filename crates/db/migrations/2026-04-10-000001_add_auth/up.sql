-- Phase 3: auth tables in the tenant DB.
--
-- `app_user` is the auth identity (email + password hash). Separate
-- from `rower` so non-rowing users (Program Directors) have accounts.
-- Linked via rower.user_id FK.

CREATE TABLE app_user (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT,             -- NULL until invite is accepted
    name TEXT NOT NULL,
    status TEXT CHECK( status IN ('invited','active','disabled') ) NOT NULL DEFAULT 'invited',
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

-- Tenant-wide role. A user has exactly one role row.
-- Team-level scoping is via team_membership / team_coach.
CREATE TABLE user_role (
    user_id INTEGER PRIMARY KEY NOT NULL,
    role TEXT CHECK( role IN ('Member','Coach','ProgramDirector') ) NOT NULL,
    FOREIGN KEY (user_id) REFERENCES app_user(id) ON DELETE CASCADE
);

-- One-time invite tokens for account activation.
CREATE TABLE user_invite (
    token_hash TEXT PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    FOREIGN KEY (user_id) REFERENCES app_user(id) ON DELETE CASCADE
);

-- Link rower → user (nullable; not every rower has an account yet).
ALTER TABLE rower ADD COLUMN user_id INTEGER REFERENCES app_user(id) ON DELETE SET NULL;
