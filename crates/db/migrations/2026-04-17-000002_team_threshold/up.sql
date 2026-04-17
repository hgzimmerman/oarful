CREATE TABLE team_threshold (
    team_id INTEGER NOT NULL,
    metric TEXT NOT NULL,          -- 'weight', 'height', 'strength'
    low_mid REAL NOT NULL,         -- boundary between bucket 1 and 2
    mid_high REAL NOT NULL,        -- boundary between bucket 2 and 3
    high_very REAL NOT NULL,       -- boundary between bucket 3 and 4
    PRIMARY KEY (team_id, metric),
    FOREIGN KEY (team_id) REFERENCES team(id)
);

-- Which erg test distance the team uses for strength bucketing.
ALTER TABLE team ADD COLUMN erg_threshold_distance_m INTEGER;
