CREATE TABLE magic_link (
    token_hash TEXT PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL,
    redirect_path TEXT NOT NULL,
    expires_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (user_id) REFERENCES app_user(id) ON DELETE CASCADE
);
