-- SQLite can't DROP COLUMN easily; this is a best-effort rollback.
-- The column will remain but default to 0 (not archived).
UPDATE team SET archived = 0;
