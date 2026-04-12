-- SQLite < 3.35 can't DROP COLUMN; recreate.
PRAGMA foreign_keys = OFF;

CREATE TABLE tenant_backup (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    db_path TEXT NOT NULL UNIQUE,
    created_at DATETIME NOT NULL,
    attributes_public INTEGER NOT NULL DEFAULT 0,
    force_cox_stern INTEGER NOT NULL DEFAULT 0
);

INSERT INTO tenant_backup SELECT id, name, slug, db_path, created_at, attributes_public, force_cox_stern FROM tenant;
DROP TABLE tenant;
ALTER TABLE tenant_backup RENAME TO tenant;

PRAGMA foreign_keys = ON;
