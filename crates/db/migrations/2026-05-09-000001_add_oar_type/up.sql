-- TODO: when squashing migrations, add CHECK(oar_type IN ('sweep', 'sculling'))
-- to the column definition. SQLite doesn't support adding CHECK via ALTER TABLE.
ALTER TABLE oar_set ADD COLUMN oar_type TEXT NOT NULL DEFAULT 'sweep';
