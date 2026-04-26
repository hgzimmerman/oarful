-- Squashed migration: final schema as of 2026-04-26.
-- Replaces ~37 incremental migrations.

CREATE TABLE rower (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    weight_class TEXT NOT NULL CHECK (weight_class IN ('Light', 'Medium', 'Heavy', 'VeryHeavy')),
    skill TEXT NOT NULL CHECK (skill IN ('Novice', 'Intermediate', 'Master', 'Expert')),
    strength TEXT NOT NULL CHECK (strength IN ('Weak', 'Intermediate', 'Strong', 'VeryStrong')),
    height TEXT NOT NULL DEFAULT 'Medium' CHECK (height IN ('Short', 'Medium', 'Tall', 'VeryTall')),
    side TEXT NOT NULL DEFAULT 'Either' CHECK (side IN ('Port', 'Starboard', 'Either')),
    side_strength INTEGER NOT NULL DEFAULT 3 CHECK (side_strength BETWEEN 0 AND 5),
    sweep_bias INTEGER NOT NULL DEFAULT 0 CHECK (sweep_bias BETWEEN -2 AND 2),
    can_cox INTEGER NOT NULL DEFAULT 0 CHECK (can_cox IN (0, 1)),
    is_designated_cox INTEGER NOT NULL DEFAULT 0 CHECK (is_designated_cox IN (0, 1)),
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    weight_kg REAL,
    height_m REAL
);

CREATE TABLE boat (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    weight_class TEXT NOT NULL CHECK (weight_class IN ('Light', 'Medium', 'Heavy', 'Tubby')),
    seat_count INTEGER NOT NULL CHECK (seat_count IN (1, 2, 4, 8)),
    has_cox INTEGER NOT NULL CHECK (has_cox IN (0, 1)),
    oars_per_seat INTEGER NOT NULL CHECK (oars_per_seat IN (1, 2)),
    stroke_side TEXT NOT NULL DEFAULT 'Starboard' CHECK (stroke_side IN ('Port', 'Starboard')),
    cox_position TEXT NOT NULL DEFAULT 'Stern' CHECK (cox_position IN ('Bow', 'Stern')),
    acquired_at DATE,
    manufactured_at DATE,
    relinquished_at DATE
);

CREATE TABLE team (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    created_at DATETIME NOT NULL,
    self_edit_level TEXT NOT NULL DEFAULT 'low' CHECK (self_edit_level IN ('low', 'medium', 'high')),
    default_practice_time TEXT,
    default_practice_duration_minutes INTEGER,
    default_practice_days INTEGER,
    archived INTEGER NOT NULL DEFAULT 0,
    assume_available INTEGER NOT NULL DEFAULT 0,
    erg_threshold_distance_m INTEGER,
    bucket_visibility TEXT NOT NULL DEFAULT 'off' CHECK (bucket_visibility IN ('off', 'view', 'edit')),
    member_raw_metrics INTEGER NOT NULL DEFAULT 0 CHECK (member_raw_metrics IN (0, 1))
);

CREATE TABLE team_membership (
    team_id INTEGER NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    rower_id INTEGER NOT NULL REFERENCES rower(id) ON DELETE CASCADE,
    PRIMARY KEY (team_id, rower_id)
);

CREATE TABLE team_coach (
    team_id INTEGER NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL,
    PRIMARY KEY (team_id, user_id)
);

CREATE TABLE team_boat_default (
    team_id INTEGER NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    boat_id INTEGER NOT NULL REFERENCES boat(id) ON DELETE CASCADE,
    PRIMARY KEY (team_id, boat_id)
);

CREATE TABLE team_threshold (
    team_id INTEGER NOT NULL REFERENCES team(id),
    metric TEXT NOT NULL,
    low_mid REAL NOT NULL,
    mid_high REAL NOT NULL,
    high_very REAL NOT NULL,
    PRIMARY KEY (team_id, metric)
);

CREATE TABLE practice (
    id INTEGER PRIMARY KEY NOT NULL,
    team_id INTEGER NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    date DATE NOT NULL,
    time TEXT,
    duration_minutes INTEGER,
    notes TEXT,
    cancelled INTEGER NOT NULL DEFAULT 0,
    UNIQUE (team_id, date, time)
);

CREATE TABLE availability (
    rower_id INTEGER NOT NULL REFERENCES rower(id) ON DELETE CASCADE,
    practice_id INTEGER NOT NULL REFERENCES practice(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'Yes',
    PRIMARY KEY (rower_id, practice_id)
);

CREATE TABLE lineup (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    practice_id INTEGER NOT NULL REFERENCES practice(id) ON DELETE CASCADE,
    boat_id INTEGER NOT NULL REFERENCES boat(id),
    created_at DATETIME NOT NULL,
    is_draft INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE lineup_seat (
    lineup_id INTEGER NOT NULL REFERENCES lineup(id) ON DELETE CASCADE,
    seat_position INTEGER NOT NULL,
    rower_id INTEGER NOT NULL REFERENCES rower(id),
    is_cox INTEGER NOT NULL CHECK (is_cox IN (0, 1)),
    PRIMARY KEY (lineup_id, seat_position)
);

CREATE INDEX idx_lineup_practice ON lineup(practice_id);
CREATE INDEX idx_lineup_seat_cox ON lineup_seat(rower_id, is_cox);

CREATE TABLE pair_affinity (
    rower_a_id INTEGER NOT NULL REFERENCES rower(id) ON DELETE CASCADE,
    rower_b_id INTEGER NOT NULL REFERENCES rower(id) ON DELETE CASCADE,
    weight INTEGER NOT NULL CHECK (weight BETWEEN -5 AND 5 AND weight != 0),
    PRIMARY KEY (rower_a_id, rower_b_id),
    CHECK (rower_a_id < rower_b_id)
);

CREATE TABLE rower_seat_affinity (
    rower_id INTEGER NOT NULL REFERENCES rower(id) ON DELETE CASCADE,
    zone TEXT NOT NULL CHECK (zone IN ('Stroke', 'SternPair', 'SternHalf', 'EngineRoom', 'BowHalf', 'BowPair', 'Bow')),
    weight INTEGER NOT NULL CHECK (weight BETWEEN -5 AND 5 AND weight != 0),
    PRIMARY KEY (rower_id, zone)
);

CREATE TABLE app_user (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'invited' CHECK (status IN ('invited', 'active', 'disabled')),
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    rower_id INTEGER REFERENCES rower(id) ON DELETE SET NULL,
    opt_in_reminders INTEGER NOT NULL DEFAULT 1,
    opt_in_lineups INTEGER NOT NULL DEFAULT 1,
    opt_in_stale_alerts INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE user_role (
    user_id INTEGER PRIMARY KEY NOT NULL REFERENCES app_user(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('Member', 'Coach', 'ProgramDirector'))
);

CREATE TABLE user_invite (
    token_hash TEXT PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL UNIQUE REFERENCES app_user(id) ON DELETE CASCADE,
    expires_at DATETIME NOT NULL
);

CREATE TABLE magic_link (
    token_hash TEXT PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL REFERENCES app_user(id) ON DELETE CASCADE,
    team_id INTEGER,
    redirect_path TEXT NOT NULL,
    expires_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL
);

CREATE TABLE solver_profile (
    id INTEGER PRIMARY KEY NOT NULL,
    team_id INTEGER NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    skill_variance_weight INTEGER NOT NULL,
    pair_affinity_weight INTEGER NOT NULL,
    seat_affinity_weight INTEGER NOT NULL,
    side_preference_weight INTEGER NOT NULL,
    weight_class_slack_weight INTEGER NOT NULL,
    cox_cooldown_penalty INTEGER NOT NULL,
    placement_reward_weight INTEGER NOT NULL,
    pair_strength_weight INTEGER NOT NULL,
    bow_pair_strength_weight INTEGER NOT NULL,
    height_balance_weight INTEGER NOT NULL,
    end_pair_skill_weight INTEGER NOT NULL,
    engine_room_strength_weight INTEGER NOT NULL,
    partial_fill_bonus INTEGER NOT NULL,
    non_scull_retention_weight INTEGER NOT NULL,
    bow_cox_fit_weight INTEGER NOT NULL,
    top_boat_stacking_weight INTEGER NOT NULL DEFAULT 0,
    pair_eligibility_weight INTEGER NOT NULL DEFAULT 3,
    minimize_bench_weight INTEGER NOT NULL DEFAULT 4,
    boat_size_stacking_weight INTEGER NOT NULL DEFAULT 0,
    eight_bias INTEGER NOT NULL DEFAULT 0,
    coxed_four_bias INTEGER NOT NULL DEFAULT 0,
    four_bias INTEGER NOT NULL DEFAULT 0,
    quad_bias INTEGER NOT NULL DEFAULT 0,
    pair_bias INTEGER NOT NULL DEFAULT 0,
    double_bias INTEGER NOT NULL DEFAULT 0,
    single_bias INTEGER NOT NULL DEFAULT 0,
    bench_cooldown_penalty INTEGER NOT NULL DEFAULT 2,
    stroke_spread_weight INTEGER NOT NULL DEFAULT 2,
    UNIQUE (team_id, name)
);

CREATE TABLE sync_source (
    id INTEGER PRIMARY KEY NOT NULL,
    team_id INTEGER NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    source_type TEXT NOT NULL CHECK (source_type IN ('google_sheet')),
    config TEXT NOT NULL,
    last_synced_at TIMESTAMP,
    last_error TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    poll_interval_minutes INTEGER
);

CREATE TABLE email_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    team_id INTEGER NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    email_type TEXT NOT NULL,
    practice_date DATE NOT NULL,
    sent_at DATETIME NOT NULL,
    recipient_count INTEGER NOT NULL,
    sent_by_user_id INTEGER NOT NULL REFERENCES app_user(id) ON DELETE CASCADE
);

CREATE INDEX idx_email_log_team_type_date ON email_log(team_id, email_type, practice_date);

CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    timestamp DATETIME NOT NULL,
    user_id INTEGER REFERENCES app_user(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    detail TEXT
);

CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp);
CREATE INDEX idx_audit_log_action ON audit_log(action, timestamp);
CREATE INDEX idx_audit_log_resource ON audit_log(resource_type, resource_id, timestamp);

CREATE TABLE erg_test (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    rower_id INTEGER NOT NULL REFERENCES rower(id),
    distance_m INTEGER NOT NULL,
    time_cs INTEGER NOT NULL,
    rowed_at DATE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_erg_test_rower ON erg_test(rower_id);

CREATE TABLE stale_digest_log (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    last_sent_at DATETIME NOT NULL
);

-- Seed: every tenant needs a default team (ID 1).
INSERT INTO team (id, name, created_at)
VALUES (1, 'Default', datetime('now'));
