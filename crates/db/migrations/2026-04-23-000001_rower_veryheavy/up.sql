-- Add 'VeryHeavy' to the rower weight_class CHECK constraint.
-- SQLite requires table recreation to alter CHECK constraints.

PRAGMA foreign_keys = OFF;

CREATE TABLE rower_new (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    weight_class TEXT CHECK( weight_class IN ('Light','Medium','Heavy','VeryHeavy') ) NOT NULL,
    skill TEXT CHECK( skill IN ('Novice','Intermediate','Master','Expert') ) NOT NULL,
    strength TEXT CHECK( strength IN ('Weak','Intermediate','Strong','VeryStrong') ) NOT NULL,
    height TEXT CHECK( height IN ('Short','Medium','Tall','VeryTall') ) NOT NULL DEFAULT 'Medium',
    side TEXT CHECK( side IN ('Port','Starboard','Either') ) NOT NULL DEFAULT 'Either',
    side_strength INTEGER CHECK( side_strength BETWEEN 0 AND 5 ) NOT NULL DEFAULT 3,
    sweep_bias INTEGER CHECK( sweep_bias BETWEEN -2 AND 2 ) NOT NULL DEFAULT 0,
    can_cox INTEGER CHECK( can_cox IN (0,1) ) NOT NULL DEFAULT 0,
    is_designated_cox INTEGER CHECK( is_designated_cox IN (0,1) ) NOT NULL DEFAULT 0,
    active INTEGER CHECK( active IN (0,1) ) NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    weight_kg REAL,
    height_m REAL
);

INSERT INTO rower_new (id, name, weight_class, skill, strength, height, side,
    side_strength, sweep_bias, can_cox, is_designated_cox, active,
    created_at, updated_at, weight_kg, height_m)
SELECT id, name, weight_class, skill, strength, height, side,
    side_strength, sweep_bias, can_cox, is_designated_cox, active,
    created_at, updated_at, weight_kg, height_m
FROM rower;

DROP TABLE rower;
ALTER TABLE rower_new RENAME TO rower;

PRAGMA foreign_keys = ON;
