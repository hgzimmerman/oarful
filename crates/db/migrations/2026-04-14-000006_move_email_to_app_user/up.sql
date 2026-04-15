-- Move email ownership from rower to app_user.
-- app_user.rower_id replaces rower.user_id (reverses the FK direction).
-- rower loses its email and user_id columns.

PRAGMA foreign_keys = OFF;

-- 1. Add rower_id to app_user.
ALTER TABLE app_user ADD COLUMN rower_id INTEGER REFERENCES rower(id) ON DELETE SET NULL;

-- 2. Populate from existing rower.user_id links.
UPDATE app_user SET rower_id = (
    SELECT rower.id FROM rower WHERE rower.user_id = app_user.id
);

-- 3. Recreate rower without email and user_id.
CREATE TABLE rower_new (
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

INSERT INTO rower_new (id, name, weight_class, skill, strength, height, side,
    side_strength, can_scull, can_cox, is_designated_cox, active, created_at, updated_at)
SELECT id, name, weight_class, skill, strength, height, side,
    side_strength, can_scull, can_cox, is_designated_cox, active, created_at, updated_at
FROM rower;

DROP TABLE rower;
ALTER TABLE rower_new RENAME TO rower;

-- Recreate indexes that reference rower (FKs are recreated by the table def above).
-- The idx_lineup_seat_cox index is on lineup_seat, not rower, so it
-- survives the rower table recreation. Use IF NOT EXISTS to be safe.
CREATE INDEX IF NOT EXISTS idx_lineup_seat_cox ON lineup_seat(rower_id, is_cox);

PRAGMA foreign_keys = ON;
