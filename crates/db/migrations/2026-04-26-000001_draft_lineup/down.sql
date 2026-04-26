-- SQLite does not support DROP COLUMN; recreate without is_draft.
CREATE TABLE lineup_backup (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    practice_id INTEGER NOT NULL REFERENCES practice(id) ON DELETE CASCADE,
    boat_id INTEGER NOT NULL REFERENCES boat(id),
    created_at DATETIME NOT NULL
);
INSERT INTO lineup_backup SELECT id, practice_id, boat_id, created_at FROM lineup;
DROP TABLE lineup;
ALTER TABLE lineup_backup RENAME TO lineup;
