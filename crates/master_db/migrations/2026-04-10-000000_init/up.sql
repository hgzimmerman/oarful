-- Master database: global tenant registry + superuser credentials.
-- One file shared across all tenants. Tiny and rarely written.

CREATE TABLE tenant (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    db_path TEXT NOT NULL UNIQUE,
    created_at DATETIME NOT NULL
);

CREATE TABLE superuser (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at DATETIME NOT NULL
);
