-- SQLite < 3.35 can't DROP COLUMN; recreate.
PRAGMA foreign_keys = OFF;

CREATE TABLE magic_link_backup (
    token_hash TEXT PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL,
    redirect_path TEXT NOT NULL,
    expires_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (user_id) REFERENCES app_user(id) ON DELETE CASCADE
);

INSERT INTO magic_link_backup SELECT token_hash, user_id, redirect_path, expires_at, created_at FROM magic_link;
DROP TABLE magic_link;
ALTER TABLE magic_link_backup RENAME TO magic_link;

PRAGMA foreign_keys = ON;
