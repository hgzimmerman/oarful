-- Replace self_edit_level with bucket_visibility + member_raw_metrics.
-- self_edit_level column is left in place (ignored by diesel) to avoid
-- a full table recreation.

ALTER TABLE team ADD COLUMN bucket_visibility TEXT NOT NULL DEFAULT 'off'
    CHECK(bucket_visibility IN ('off', 'view', 'edit'));

ALTER TABLE team ADD COLUMN member_raw_metrics INTEGER NOT NULL DEFAULT 0
    CHECK(member_raw_metrics IN (0, 1));
