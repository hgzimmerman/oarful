-- SQLite doesn't support DROP COLUMN before 3.35.0; recreate the table.
CREATE TABLE tenant_backup AS SELECT id, name, slug, db_path, created_at, attributes_public, force_cox_stern, demo_expires_at, emails_visible FROM tenant;
DROP TABLE tenant;
ALTER TABLE tenant_backup RENAME TO tenant;
