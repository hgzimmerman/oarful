-- Phase 1 of auth/multi-tenancy: team structure.
--
-- Adds team, team_membership, team_coach tables. Migrates practice
-- and availability to carry team_id. Seeds a "Default" team and
-- assigns all existing data to it so the migration is non-destructive.
--
-- SQLite can't ALTER a PRIMARY KEY or add a NOT NULL FK to an existing
-- table, so practice and availability are recreated via the standard
-- copy-drop-rename dance.

PRAGMA foreign_keys = OFF;

-- =====================================================================
-- New tables
-- =====================================================================

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
-- user_id references the future `user` table (Phase 3). For now
-- the column is an integer with no FK — we'll add the constraint
-- when the user table lands.
CREATE TABLE team_coach (
    team_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    PRIMARY KEY (team_id, user_id),
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE
);

-- =====================================================================
-- Seed a default team so existing data has somewhere to go
-- =====================================================================

INSERT INTO team (id, name, created_at)
VALUES (1, 'Default', datetime('now'));

-- Put all existing active rowers into the default team.
INSERT INTO team_membership (team_id, rower_id)
SELECT 1, id FROM rower WHERE active = 1;

-- =====================================================================
-- Recreate practice with team_id
-- =====================================================================

CREATE TABLE practice_new (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    team_id INTEGER NOT NULL,
    date DATE NOT NULL,
    notes TEXT,
    -- Two teams can practice on the same date; same team can't
    -- practice twice on the same date.
    UNIQUE (team_id, date),
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE
);

INSERT INTO practice_new (id, team_id, date, notes)
SELECT id, 1, date, notes FROM practice;

-- Drop tables that FK to practice (lineup → practice) before
-- dropping practice itself. We recreate them pointing at practice_new.
-- lineup_seat FKs to lineup, so it goes first.

CREATE TABLE lineup_new (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    practice_id INTEGER NOT NULL,
    boat_id INTEGER NOT NULL,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (practice_id) REFERENCES practice_new(id) ON DELETE CASCADE,
    FOREIGN KEY (boat_id) REFERENCES boat(id)
);

INSERT INTO lineup_new SELECT * FROM lineup;

CREATE TABLE lineup_seat_new (
    lineup_id INTEGER NOT NULL,
    seat_position INTEGER NOT NULL,
    rower_id INTEGER NOT NULL,
    is_cox INTEGER CHECK( is_cox IN (0,1) ) NOT NULL,
    PRIMARY KEY (lineup_id, seat_position),
    FOREIGN KEY (lineup_id) REFERENCES lineup_new(id) ON DELETE CASCADE,
    FOREIGN KEY (rower_id) REFERENCES rower(id)
);

INSERT INTO lineup_seat_new SELECT * FROM lineup_seat;

DROP TABLE lineup_seat;
DROP TABLE lineup;
DROP TABLE practice;

ALTER TABLE practice_new RENAME TO practice;
ALTER TABLE lineup_new RENAME TO lineup;
ALTER TABLE lineup_seat_new RENAME TO lineup_seat;

CREATE INDEX idx_lineup_practice ON lineup(practice_id);
CREATE INDEX idx_lineup_seat_cox ON lineup_seat(rower_id, is_cox);

-- =====================================================================
-- Recreate availability with team_id in the PK
-- =====================================================================

CREATE TABLE availability_new (
    rower_id INTEGER NOT NULL,
    team_id INTEGER NOT NULL,
    date DATE NOT NULL,
    status TEXT CHECK( status IN ('Yes','No','Maybe','ScullingOnly') ) NOT NULL,
    PRIMARY KEY (rower_id, team_id, date),
    FOREIGN KEY (rower_id) REFERENCES rower(id) ON DELETE CASCADE,
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE
);

INSERT INTO availability_new (rower_id, team_id, date, status)
SELECT rower_id, 1, date, status FROM availability;

DROP TABLE availability;
ALTER TABLE availability_new RENAME TO availability;

CREATE INDEX idx_availability_date ON availability(date);
CREATE INDEX idx_availability_team_date ON availability(team_id, date);

PRAGMA foreign_keys = ON;
