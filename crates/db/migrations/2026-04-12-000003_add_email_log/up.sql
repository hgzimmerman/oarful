CREATE TABLE email_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    team_id INTEGER NOT NULL,
    email_type TEXT NOT NULL,
    practice_date DATE NOT NULL,
    sent_at DATETIME NOT NULL,
    recipient_count INTEGER NOT NULL,
    sent_by_user_id INTEGER NOT NULL,
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE,
    FOREIGN KEY (sent_by_user_id) REFERENCES app_user(id) ON DELETE CASCADE
);

CREATE INDEX idx_email_log_team_type_date ON email_log(team_id, email_type, practice_date);
