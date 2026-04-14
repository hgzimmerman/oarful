-- Rebuild practice table to add nullable time column and update unique constraint.
-- SQLite doesn't support ALTER TABLE ADD with constraint changes.
CREATE TABLE practice_new (
    id          INTEGER PRIMARY KEY NOT NULL,
    team_id     INTEGER NOT NULL,
    date        DATE NOT NULL,
    time        TEXT,
    notes       TEXT,
    cancelled   INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE,
    UNIQUE (team_id, date, time)
);
INSERT INTO practice_new (id, team_id, date, notes, cancelled)
    SELECT id, team_id, date, notes, cancelled FROM practice;
DROP TABLE practice;
ALTER TABLE practice_new RENAME TO practice;
