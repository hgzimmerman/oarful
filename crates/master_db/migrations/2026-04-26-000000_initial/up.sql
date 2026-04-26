-- Squashed migration: final master DB schema as of 2026-04-26.
-- Replaces ~9 incremental migrations.

CREATE TABLE tenant (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    db_path TEXT NOT NULL UNIQUE,
    created_at DATETIME NOT NULL,
    attributes_public INTEGER NOT NULL DEFAULT 0 CHECK (attributes_public IN (0, 1)),
    force_cox_stern INTEGER NOT NULL DEFAULT 0 CHECK (force_cox_stern IN (0, 1)),
    emails_visible INTEGER NOT NULL DEFAULT 0,
    demo_expires_at DATETIME,
    billing_status TEXT NOT NULL DEFAULT 'trial',
    stripe_customer_id TEXT,
    stripe_subscription_id TEXT
);
