-- SQLite 3.35+ supports DROP COLUMN.
ALTER TABLE rower DROP COLUMN user_id;
DROP TABLE IF EXISTS user_invite;
DROP TABLE IF EXISTS user_role;
DROP TABLE IF EXISTS app_user;
