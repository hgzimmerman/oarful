CREATE TABLE onboarding_progress (
    app_user_id INTEGER NOT NULL REFERENCES app_user(id),
    step TEXT NOT NULL,
    completed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (app_user_id, step)
);
