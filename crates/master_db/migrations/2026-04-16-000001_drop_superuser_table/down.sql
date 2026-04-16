CREATE TABLE superuser (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at DATETIME NOT NULL
);
