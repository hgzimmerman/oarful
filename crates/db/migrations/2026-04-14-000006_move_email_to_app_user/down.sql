-- Reverse: restore email + user_id to rower, remove rower_id from app_user.
PRAGMA foreign_keys = OFF;

CREATE TABLE rower_old (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    weight_class TEXT CHECK( weight_class IN ('Light','Medium','Heavy') ) NOT NULL,
    skill TEXT CHECK( skill IN ('Novice','Intermediate','Master','Expert') ) NOT NULL,
    strength TEXT CHECK( strength IN ('Weak','Intermediate','Strong','VeryStrong') ) NOT NULL,
    height TEXT CHECK( height IN ('Short','Medium','Tall','VeryTall') ) NOT NULL DEFAULT 'Medium',
    side TEXT CHECK( side IN ('Port','Starboard','Either') ) NOT NULL DEFAULT 'Either',
    side_strength INTEGER CHECK( side_strength BETWEEN 0 AND 5 ) NOT NULL DEFAULT 3,
    can_scull INTEGER CHECK( can_scull IN (0,1) ) NOT NULL DEFAULT 0,
    can_cox INTEGER CHECK( can_cox IN (0,1) ) NOT NULL DEFAULT 0,
    is_designated_cox INTEGER CHECK( is_designated_cox IN (0,1) ) NOT NULL DEFAULT 0,
    active INTEGER CHECK( active IN (0,1) ) NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    user_id INTEGER REFERENCES app_user(id) ON DELETE SET NULL
);

INSERT INTO rower_old (id, name, weight_class, skill, strength, height, side,
    side_strength, can_scull, can_cox, is_designated_cox, active, created_at, updated_at)
SELECT id, name, weight_class, skill, strength, height, side,
    side_strength, can_scull, can_cox, is_designated_cox, active, created_at, updated_at
FROM rower;

-- Restore user_id from app_user.rower_id
UPDATE rower_old SET user_id = (
    SELECT app_user.id FROM app_user WHERE app_user.rower_id = rower_old.id
);

-- Restore email from app_user.email where linked
UPDATE rower_old SET email = (
    SELECT app_user.email FROM app_user WHERE app_user.rower_id = rower_old.id
);

DROP TABLE rower;
ALTER TABLE rower_old RENAME TO rower;

-- Remove rower_id from app_user (SQLite needs table recreation)
CREATE TABLE app_user_new (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'invited',
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    opt_in_reminders INTEGER NOT NULL DEFAULT 1,
    opt_in_lineups INTEGER NOT NULL DEFAULT 1
);

INSERT INTO app_user_new (id, email, password_hash, name, status, created_at, updated_at, opt_in_reminders, opt_in_lineups)
SELECT id, email, password_hash, name, status, created_at, updated_at, opt_in_reminders, opt_in_lineups
FROM app_user;

DROP TABLE app_user;
ALTER TABLE app_user_new RENAME TO app_user;

CREATE INDEX idx_lineup_seat_cox ON lineup_seat(rower_id, is_cox);

PRAGMA foreign_keys = ON;
