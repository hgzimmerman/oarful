-- SQLite can't DROP COLUMN in older versions; recreate the table without
-- the new columns. This is a destructive rollback.

PRAGMA foreign_keys = OFF;

CREATE TABLE team_old AS SELECT id, name, created_at, self_edit_level,
    default_practice_time, default_practice_duration_minutes, archived,
    default_practice_days, assume_available, erg_threshold_distance_m
FROM team;
DROP TABLE team;
ALTER TABLE team_old RENAME TO team;

PRAGMA foreign_keys = ON;
