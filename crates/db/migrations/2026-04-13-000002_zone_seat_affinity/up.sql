-- Zone-based seat affinity replaces absolute seat positions.
-- Existing seat_position data cannot be losslessly mapped to zones,
-- so we drop and recreate the table.
DROP TABLE IF EXISTS rower_seat_affinity;

CREATE TABLE rower_seat_affinity (
    rower_id INTEGER NOT NULL,
    zone TEXT CHECK( zone IN ('Stroke','SternPair','SternHalf','EngineRoom','BowHalf','BowPair','Bow') ) NOT NULL,
    weight INTEGER CHECK( weight BETWEEN -5 AND 5 AND weight != 0 ) NOT NULL,
    PRIMARY KEY (rower_id, zone),
    FOREIGN KEY (rower_id) REFERENCES rower(id) ON DELETE CASCADE
);
