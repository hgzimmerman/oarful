-- Reverse: restore can_scull, drop sweep_bias.
PRAGMA foreign_keys = OFF;

CREATE TABLE rower_old (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
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
    updated_at DATETIME NOT NULL
);

INSERT INTO rower_old (id, name, weight_class, skill, strength, height, side,
    side_strength, can_scull, can_cox, is_designated_cox, active, created_at, updated_at)
SELECT id, name, weight_class, skill, strength, height, side,
    side_strength,
    CASE WHEN sweep_bias <= 0 THEN 1 ELSE 0 END,
    can_cox, is_designated_cox, active, created_at, updated_at
FROM rower;

DROP TABLE rower;
ALTER TABLE rower_old RENAME TO rower;

CREATE INDEX IF NOT EXISTS idx_lineup_seat_cox ON lineup_seat(rower_id, is_cox);

PRAGMA foreign_keys = ON;
