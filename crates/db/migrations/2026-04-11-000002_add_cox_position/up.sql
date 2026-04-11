ALTER TABLE boat ADD COLUMN cox_position TEXT NOT NULL DEFAULT 'Stern' CHECK(cox_position IN ('Bow', 'Stern'));
