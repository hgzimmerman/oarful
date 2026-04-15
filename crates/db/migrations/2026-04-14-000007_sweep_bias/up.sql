-- Replace can_scull boolean with sweep_bias (-2..2) on rower.
-- Simplify availability to Yes/No (drop Maybe, ScullingOnly).

PRAGMA foreign_keys = OFF;

-- 1. Convert availability statuses before tightening the enum.
UPDATE availability SET status = 'Yes' WHERE status = 'ScullingOnly';
UPDATE availability SET status = 'No'  WHERE status = 'Maybe';

-- 2. Recreate rower: drop can_scull, add sweep_bias.
CREATE TABLE rower_new (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    weight_class TEXT CHECK( weight_class IN ('Light','Medium','Heavy') ) NOT NULL,
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
    updated_at DATETIME NOT NULL
);

-- Populate: can_scull=false → sweep_bias=2 (sweep-only),
--           can_scull=true  → sweep_bias=0 (ambivalent).
-- Rowers who had ScullingOnly availability get -2 (hard sculler).
INSERT INTO rower_new (id, name, weight_class, skill, strength, height, side,
    side_strength, sweep_bias, can_cox, is_designated_cox, active, created_at, updated_at)
SELECT id, name, weight_class, skill, strength, height, side,
    side_strength,
    CASE
        WHEN can_scull = 0 THEN 2
        ELSE 0
    END,
    can_cox, is_designated_cox, active, created_at, updated_at
FROM rower;

DROP TABLE rower;
ALTER TABLE rower_new RENAME TO rower;

CREATE INDEX IF NOT EXISTS idx_lineup_seat_cox ON lineup_seat(rower_id, is_cox);

PRAGMA foreign_keys = ON;
