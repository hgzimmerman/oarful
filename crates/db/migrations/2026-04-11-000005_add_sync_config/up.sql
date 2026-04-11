CREATE TABLE sync_source (
    id          INTEGER PRIMARY KEY NOT NULL,
    team_id     INTEGER NOT NULL,
    source_type TEXT    NOT NULL CHECK(source_type IN ('google_sheet')),
    -- Source-specific config as JSON. For google_sheet:
    -- {"sheet_id": "...", "gid": 0}
    config      TEXT    NOT NULL,
    last_synced_at TIMESTAMP,
    last_error     TEXT,
    created_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE
);
