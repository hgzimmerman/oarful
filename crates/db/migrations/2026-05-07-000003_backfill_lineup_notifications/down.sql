-- Remove all backfilled rows. Safe because lineup_notification was empty
-- before this migration.
DELETE FROM lineup_notification;
