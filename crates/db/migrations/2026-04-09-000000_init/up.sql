PRAGMA foreign_keys = ON;

-- A rower on the team. Deliberately coarse: every measurable trait is a
-- bucketed enum so an admin can enter a new rower in seconds. Anything numeric
-- (exact weight, erg time) is intentionally absent — the buckets carry enough
-- signal for the solver. `last_coxed_on` is NOT stored; it is derived from
-- the `lineup_seat` history.
--
-- Scope note: this project only generates SWEEP lineups. Scullers are a
-- separate team. `can_scull` is kept as an eligibility flag so the solver can
-- recommend "push this rower to the scullers" as an overflow when no sweep
-- seat fits them — it is not used to assign them to scull boats here.
CREATE TABLE rower (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL UNIQUE,
    weight_class TEXT CHECK( weight_class IN ('Light','Medium','Heavy') ) NOT NULL,
    skill TEXT CHECK( skill IN ('Novice','Intermediate','Master','Expert') ) NOT NULL,
    strength TEXT CHECK( strength IN ('Weak','Intermediate','Strong','VeryStrong') ) NOT NULL,
    side TEXT CHECK( side IN ('Port','Starboard','Either') ) NOT NULL DEFAULT 'Either',
    -- 0 = hard side-lock (cannot row the opposite side at all);
    -- 1..5 = soft preference, scales the S4 wrong-side penalty.
    side_strength INTEGER CHECK( side_strength BETWEEN 0 AND 5 ) NOT NULL DEFAULT 3,
    can_scull INTEGER CHECK( can_scull IN (0,1) ) NOT NULL DEFAULT 0,
    can_cox INTEGER CHECK( can_cox IN (0,1) ) NOT NULL DEFAULT 0,
    is_designated_cox INTEGER CHECK( is_designated_cox IN (0,1) ) NOT NULL DEFAULT 0,
    active INTEGER CHECK( active IN (0,1) ) NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

-- Boats in the fleet. Schema mirrors the sibling `boat_tracking` project so
-- the two apps can share a fleet snapshot cleanly in the future. Boats are a
-- thin reference entity here — the point is to have something for lineups to
-- foreign-key to, not to manage maintenance / usage.
CREATE TABLE boat (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    name TEXT NOT NULL,
    weight_class TEXT CHECK( weight_class IN ('Light','Medium','Heavy','Tubby') ) NOT NULL,
    seat_count INTEGER CHECK( seat_count IN (1,2,4,8) ) NOT NULL,
    has_cox INTEGER CHECK( has_cox IN (0,1) ) NOT NULL,
    oars_per_seat INTEGER CHECK( oars_per_seat IN (1,2) ) NOT NULL,
    acquired_at DATE,
    manufactured_at DATE,
    relinquished_at DATE
);

-- How much a rower likes a particular seat position (0 = cox, 1 = bow, n = stroke).
-- Positive weight = wants the seat; negative = avoid.
-- Weight is bounded ±5 and must be nonzero. A zero weight is meaningless
-- ("no preference") and equivalent to the row not existing; forbidding it
-- at the SQL level lets the solver skip a defensive Pumpkin-scaled(0) check.
CREATE TABLE rower_seat_affinity (
    rower_id INTEGER NOT NULL,
    seat_position INTEGER NOT NULL,
    weight INTEGER CHECK( weight BETWEEN -5 AND 5 AND weight != 0 ) NOT NULL,
    PRIMARY KEY (rower_id, seat_position),
    FOREIGN KEY (rower_id) REFERENCES rower(id) ON DELETE CASCADE
);

-- Pair affinities / anti-affinities. Canonicalised so rower_a_id < rower_b_id
-- to avoid double-storing the symmetric relationship. Weight is bounded ±5
-- and must be nonzero — same reasoning as rower_seat_affinity.
CREATE TABLE pair_affinity (
    rower_a_id INTEGER NOT NULL,
    rower_b_id INTEGER NOT NULL,
    weight INTEGER CHECK( weight BETWEEN -5 AND 5 AND weight != 0 ) NOT NULL,
    PRIMARY KEY (rower_a_id, rower_b_id),
    CHECK (rower_a_id < rower_b_id),
    FOREIGN KEY (rower_a_id) REFERENCES rower(id) ON DELETE CASCADE,
    FOREIGN KEY (rower_b_id) REFERENCES rower(id) ON DELETE CASCADE
);

CREATE TABLE practice (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    date DATE NOT NULL UNIQUE,
    notes TEXT
);

-- Availability captures the shared-spreadsheet response for each rower-date.
-- `ScullingOnly` means the rower is attending as part of the scullers team
-- that day: we still want them in the system (their attendance is synced
-- from the same sheet) but the sweep solver excludes them from evaluation.
-- `Maybe` is a soft "show-up depends" that the coach can promote.
CREATE TABLE availability (
    rower_id INTEGER NOT NULL,
    date DATE NOT NULL,
    status TEXT CHECK( status IN ('Yes','No','Maybe','ScullingOnly') ) NOT NULL,
    PRIMARY KEY (rower_id, date),
    FOREIGN KEY (rower_id) REFERENCES rower(id) ON DELETE CASCADE
);

-- A committed lineup: one boat fielded at one practice, with its seat assignments.
CREATE TABLE lineup (
    id INTEGER PRIMARY KEY ASC NOT NULL,
    practice_id INTEGER NOT NULL,
    boat_id INTEGER NOT NULL,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (practice_id) REFERENCES practice(id) ON DELETE CASCADE,
    FOREIGN KEY (boat_id) REFERENCES boat(id)
);

CREATE TABLE lineup_seat (
    lineup_id INTEGER NOT NULL,
    seat_position INTEGER NOT NULL,               -- 0 = cox, 1 = bow, n = stroke
    rower_id INTEGER NOT NULL,
    is_cox INTEGER CHECK( is_cox IN (0,1) ) NOT NULL,
    PRIMARY KEY (lineup_id, seat_position),
    FOREIGN KEY (lineup_id) REFERENCES lineup(id) ON DELETE CASCADE,
    FOREIGN KEY (rower_id) REFERENCES rower(id)
);

CREATE INDEX idx_availability_date ON availability(date);
CREATE INDEX idx_lineup_practice ON lineup(practice_id);
-- Supports "when did this rower last cox?" derived from lineup history.
CREATE INDEX idx_lineup_seat_cox ON lineup_seat(rower_id, is_cox);
