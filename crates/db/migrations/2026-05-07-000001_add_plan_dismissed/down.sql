-- SQLite does not support DROP COLUMN on older versions; this is best-effort.
ALTER TABLE practice DROP COLUMN plan_dismissed;
