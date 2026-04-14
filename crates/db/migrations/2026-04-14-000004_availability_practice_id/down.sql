-- Reverse: re-key availability back to (rower_id, team_id, date).
CREATE TABLE availability_old (
    rower_id  INTEGER NOT NULL,
    team_id   INTEGER NOT NULL,
    date      DATE NOT NULL,
    status    TEXT NOT NULL DEFAULT 'Yes',
    PRIMARY KEY (rower_id, team_id, date),
    FOREIGN KEY (rower_id) REFERENCES rower(id) ON DELETE CASCADE,
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE
);
INSERT INTO availability_old (rower_id, team_id, date, status)
    SELECT a.rower_id, p.team_id, p.date, a.status
    FROM availability a
    JOIN practice p ON p.id = a.practice_id;
DROP TABLE availability;
ALTER TABLE availability_old RENAME TO availability;
