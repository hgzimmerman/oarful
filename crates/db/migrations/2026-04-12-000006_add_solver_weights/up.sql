ALTER TABLE solver_profile ADD COLUMN pair_eligibility_weight INTEGER NOT NULL DEFAULT 3;
ALTER TABLE solver_profile ADD COLUMN minimize_bench_weight INTEGER NOT NULL DEFAULT 4;
ALTER TABLE solver_profile ADD COLUMN boat_size_stacking_weight INTEGER NOT NULL DEFAULT 0;
