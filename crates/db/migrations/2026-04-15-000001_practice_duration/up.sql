-- Add default practice duration to team and per-practice override.
-- Same pattern as default_practice_time / practice.time.

ALTER TABLE team ADD COLUMN default_practice_duration_minutes INTEGER;
ALTER TABLE practice ADD COLUMN duration_minutes INTEGER;
