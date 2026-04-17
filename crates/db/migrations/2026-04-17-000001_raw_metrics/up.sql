ALTER TABLE rower ADD COLUMN weight_kg REAL;
ALTER TABLE rower ADD COLUMN height_m REAL;

CREATE TABLE erg_test (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    rower_id INTEGER NOT NULL REFERENCES rower(id),
    distance_m INTEGER NOT NULL,
    time_cs INTEGER NOT NULL,
    rowed_at DATE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_erg_test_rower ON erg_test(rower_id);
