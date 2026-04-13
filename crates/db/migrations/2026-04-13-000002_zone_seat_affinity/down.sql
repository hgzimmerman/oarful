DROP TABLE IF EXISTS rower_seat_affinity;

CREATE TABLE rower_seat_affinity (
    rower_id INTEGER NOT NULL,
    seat_position INTEGER NOT NULL,
    weight INTEGER CHECK( weight BETWEEN -5 AND 5 AND weight != 0 ) NOT NULL,
    PRIMARY KEY (rower_id, seat_position),
    FOREIGN KEY (rower_id) REFERENCES rower(id) ON DELETE CASCADE
);
