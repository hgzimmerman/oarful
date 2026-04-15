-- SQLite can't DROP COLUMN easily; recreate tables without the new columns.
PRAGMA foreign_keys = OFF;

CREATE TABLE team_old AS SELECT id, name, created_at, self_edit_level, default_practice_time FROM team;
DROP TABLE team;
ALTER TABLE team_old RENAME TO team;

CREATE TABLE practice_old AS SELECT id, team_id, date, time, notes, cancelled FROM practice;
DROP TABLE practice;
ALTER TABLE practice_old RENAME TO practice;

PRAGMA foreign_keys = ON;
