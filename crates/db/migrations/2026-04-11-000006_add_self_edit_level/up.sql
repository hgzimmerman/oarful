ALTER TABLE team ADD COLUMN self_edit_level TEXT NOT NULL DEFAULT 'low' CHECK(self_edit_level IN ('low', 'medium', 'high'));
