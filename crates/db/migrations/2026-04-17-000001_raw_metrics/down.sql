DROP TABLE erg_test;

-- SQLite doesn't support DROP COLUMN before 3.35; these columns
-- are nullable so leaving them is harmless on older versions.
ALTER TABLE rower DROP COLUMN weight_kg;
ALTER TABLE rower DROP COLUMN height_m;
