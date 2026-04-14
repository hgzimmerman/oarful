-- Re-key availability from (rower_id, team_id, date) to (rower_id, practice_id).
-- Backfill practice_id by joining on (team_id, date). Orphan rows (no matching
-- practice) are dropped.
CREATE TABLE availability_new (
    rower_id     INTEGER NOT NULL,
    practice_id  INTEGER NOT NULL,
    status       TEXT NOT NULL DEFAULT 'Yes',
    PRIMARY KEY (rower_id, practice_id),
    FOREIGN KEY (rower_id) REFERENCES rower(id) ON DELETE CASCADE,
    FOREIGN KEY (practice_id) REFERENCES practice(id) ON DELETE CASCADE
);
INSERT INTO availability_new (rower_id, practice_id, status)
    SELECT a.rower_id, p.id, a.status
    FROM availability a
    JOIN practice p ON p.team_id = a.team_id AND p.date = a.date;
DROP TABLE availability;
ALTER TABLE availability_new RENAME TO availability;
