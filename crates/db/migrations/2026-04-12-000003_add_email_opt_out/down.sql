-- SQLite doesn't support DROP COLUMN before 3.35.0; recreate table.
PRAGMA foreign_keys = OFF;

CREATE TABLE app_user_backup (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    email TEXT NOT NULL,
    password_hash TEXT,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'invited',
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

INSERT INTO app_user_backup SELECT id, email, password_hash, name, status, created_at, updated_at FROM app_user;
DROP TABLE app_user;
ALTER TABLE app_user_backup RENAME TO app_user;

PRAGMA foreign_keys = ON;
