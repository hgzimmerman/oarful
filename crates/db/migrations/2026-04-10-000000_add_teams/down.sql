-- Reverse the team migration. Recreate practice and availability
-- without team_id, dropping team-related tables.

PRAGMA foreign_keys = OFF;

-- Recreate practice without team_id
CREATE TABLE practice_old (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    date DATE NOT NULL UNIQUE,
    notes TEXT
);

INSERT INTO practice_old (id, date, notes)
SELECT id, date, notes FROM practice;

-- Recreate lineup chain pointing at practice_old
CREATE TABLE lineup_old (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    practice_id INTEGER NOT NULL,
    boat_id INTEGER NOT NULL,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (practice_id) REFERENCES practice_old(id) ON DELETE CASCADE,
    FOREIGN KEY (boat_id) REFERENCES boat(id)
);
INSERT INTO lineup_old SELECT * FROM lineup;

CREATE TABLE lineup_seat_old (
    lineup_id INTEGER NOT NULL,
    seat_position INTEGER NOT NULL,
    rower_id INTEGER NOT NULL,
    is_cox INTEGER CHECK( is_cox IN (0,1) ) NOT NULL,
    PRIMARY KEY (lineup_id, seat_position),
    FOREIGN KEY (lineup_id) REFERENCES lineup_old(id) ON DELETE CASCADE,
    FOREIGN KEY (rower_id) REFERENCES rower(id)
);
INSERT INTO lineup_seat_old SELECT * FROM lineup_seat;

DROP TABLE lineup_seat;
DROP TABLE lineup;
DROP TABLE practice;

ALTER TABLE practice_old RENAME TO practice;
ALTER TABLE lineup_old RENAME TO lineup;
ALTER TABLE lineup_seat_old RENAME TO lineup_seat;

CREATE INDEX idx_lineup_practice ON lineup(practice_id);
CREATE INDEX idx_lineup_seat_cox ON lineup_seat(rower_id, is_cox);

-- Recreate availability without team_id
CREATE TABLE availability_old (
    rower_id INTEGER NOT NULL,
    date DATE NOT NULL,
    status TEXT CHECK( status IN ('Yes','No','Maybe','ScullingOnly') ) NOT NULL,
    PRIMARY KEY (rower_id, date),
    FOREIGN KEY (rower_id) REFERENCES rower(id) ON DELETE CASCADE
);

INSERT INTO availability_old (rower_id, date, status)
SELECT rower_id, date, status FROM availability;

DROP TABLE availability;
ALTER TABLE availability_old RENAME TO availability;

CREATE INDEX idx_availability_date ON availability(date);

-- Drop team tables
DROP TABLE IF EXISTS team_coach;
DROP TABLE IF EXISTS team_membership;
DROP TABLE IF EXISTS team;

PRAGMA foreign_keys = ON;
