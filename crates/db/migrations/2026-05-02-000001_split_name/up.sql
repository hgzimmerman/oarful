-- Add nullable first/last name columns to rower and app_user.
ALTER TABLE rower ADD COLUMN first_name TEXT;
ALTER TABLE rower ADD COLUMN last_name TEXT;

ALTER TABLE app_user ADD COLUMN first_name TEXT;
ALTER TABLE app_user ADD COLUMN last_name TEXT;

-- Split unambiguous two-word names into first/last.
-- Names with 1 word or 3+ words are left NULL for manual resolution.
UPDATE rower
   SET first_name = SUBSTR(name, 1, INSTR(name, ' ') - 1),
       last_name  = SUBSTR(name, INSTR(name, ' ') + 1)
 WHERE LENGTH(name) - LENGTH(REPLACE(name, ' ', '')) = 1;

UPDATE app_user
   SET first_name = SUBSTR(name, 1, INSTR(name, ' ') - 1),
       last_name  = SUBSTR(name, INSTR(name, ' ') + 1)
 WHERE LENGTH(name) - LENGTH(REPLACE(name, ' ', '')) = 1;
