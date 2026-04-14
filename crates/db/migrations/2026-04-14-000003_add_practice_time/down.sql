CREATE TABLE practice_old (
    id          INTEGER PRIMARY KEY NOT NULL,
    team_id     INTEGER NOT NULL,
    date        DATE NOT NULL,
    notes       TEXT,
    cancelled   INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE,
    UNIQUE (team_id, date)
);
INSERT INTO practice_old (id, team_id, date, notes, cancelled)
    SELECT id, team_id, date, notes, cancelled FROM practice;
DROP TABLE practice;
ALTER TABLE practice_old RENAME TO practice;
