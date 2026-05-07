CREATE TABLE lineup_notification (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    practice_id INTEGER NOT NULL REFERENCES practice(id),
    rower_id INTEGER NOT NULL REFERENCES rower(id),
    sent_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(practice_id, rower_id)
);
