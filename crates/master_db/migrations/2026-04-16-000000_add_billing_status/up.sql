ALTER TABLE tenant ADD COLUMN billing_status TEXT NOT NULL DEFAULT 'trial';
ALTER TABLE tenant ADD COLUMN trial_expires_at DATETIME;
