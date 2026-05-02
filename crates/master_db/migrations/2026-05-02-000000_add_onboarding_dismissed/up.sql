ALTER TABLE tenant ADD COLUMN onboarding_dismissed INTEGER NOT NULL DEFAULT 0 CHECK (onboarding_dismissed IN (0, 1));
