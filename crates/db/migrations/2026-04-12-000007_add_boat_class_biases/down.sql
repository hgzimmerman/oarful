-- SQLite < 3.35 can't DROP COLUMN; recreate without the 7 bias columns.
PRAGMA foreign_keys = OFF;

CREATE TABLE solver_profile_backup AS
    SELECT id, team_id, name, description,
           skill_variance_weight, pair_affinity_weight, seat_affinity_weight,
           side_preference_weight, weight_class_slack_weight, cox_cooldown_penalty,
           placement_reward_weight, pair_strength_weight, bow_pair_strength_weight,
           height_balance_weight, end_pair_skill_weight, engine_room_strength_weight,
           partial_fill_bonus, non_scull_retention_weight, bow_cox_fit_weight,
           top_boat_stacking_weight, pair_eligibility_weight, minimize_bench_weight,
           boat_size_stacking_weight
    FROM solver_profile;
DROP TABLE solver_profile;
ALTER TABLE solver_profile_backup RENAME TO solver_profile;

PRAGMA foreign_keys = ON;
