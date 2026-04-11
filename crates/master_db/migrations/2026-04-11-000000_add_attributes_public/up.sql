ALTER TABLE tenant ADD COLUMN attributes_public INTEGER NOT NULL DEFAULT 0 CHECK(attributes_public IN (0, 1));
