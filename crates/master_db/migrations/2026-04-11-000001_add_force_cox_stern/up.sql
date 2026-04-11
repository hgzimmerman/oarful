ALTER TABLE tenant ADD COLUMN force_cox_stern INTEGER NOT NULL DEFAULT 0 CHECK(force_cox_stern IN (0, 1));
