-- Stale lineup email alerts: opt-in field + digest tracking.

ALTER TABLE app_user ADD COLUMN opt_in_stale_alerts INTEGER NOT NULL DEFAULT 1;

CREATE TABLE stale_digest_log (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    last_sent_at DATETIME NOT NULL
);
