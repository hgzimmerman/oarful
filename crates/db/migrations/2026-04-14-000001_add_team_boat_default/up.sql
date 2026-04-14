CREATE TABLE team_boat_default (
    team_id     INTEGER NOT NULL,
    boat_id     INTEGER NOT NULL,
    PRIMARY KEY (team_id, boat_id),
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE,
    FOREIGN KEY (boat_id) REFERENCES boat(id) ON DELETE CASCADE
);
