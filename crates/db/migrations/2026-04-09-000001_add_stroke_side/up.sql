-- Add `stroke_side` to boat. A boat is described by which side the STROKE
-- seat sits on — this is the standard convention: "starboard rigged" means
-- stroke is on starboard, "port rigged" means stroke is on port. The other
-- seats alternate back toward the bow.
--
-- This assumes a standard alternating rig. Bucket / German / Italian rigs
-- that put two adjacent seats on the same side are not modelled yet; the
-- escape hatch if we need them later is a per-seat override table.
--
-- We reuse the existing `Side` enum (Port / Starboard / Either) rather than
-- introducing a boat-only two-value enum. The CHECK constraint below keeps
-- Either out at the SQL level, so invalid states are unrepresentable.
ALTER TABLE boat
    ADD COLUMN stroke_side TEXT
        CHECK( stroke_side IN ('Port','Starboard') )
        NOT NULL
        DEFAULT 'Starboard';
