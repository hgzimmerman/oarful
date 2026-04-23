//! Saved solver weight profiles (custom presets).

use crate::schema::solver_profile;
use crate::team::TeamId;
use diesel::prelude::*;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct SolverProfileId(i32);

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = solver_profile)]
pub struct SolverProfile {
    pub id: SolverProfileId,
    pub team_id: TeamId,
    pub name: String,
    pub description: Option<String>,
    pub skill_variance_weight: i32,
    pub pair_affinity_weight: i32,
    pub seat_affinity_weight: i32,
    pub side_preference_weight: i32,
    pub weight_class_slack_weight: i32,
    pub cox_cooldown_penalty: i32,
    pub placement_reward_weight: i32,
    pub pair_strength_weight: i32,
    pub bow_pair_strength_weight: i32,
    pub height_balance_weight: i32,
    pub end_pair_skill_weight: i32,
    pub engine_room_strength_weight: i32,
    pub partial_fill_bonus: i32,
    pub non_scull_retention_weight: i32,
    pub bow_cox_fit_weight: i32,
    pub top_boat_stacking_weight: i32,
    pub pair_eligibility_weight: i32,
    pub minimize_bench_weight: i32,
    pub boat_size_stacking_weight: i32,
    pub bench_cooldown_penalty: i32,
    pub stroke_spread_weight: i32,
    pub eight_bias: i32,
    pub coxed_four_bias: i32,
    pub four_bias: i32,
    pub quad_bias: i32,
    pub pair_bias: i32,
    pub double_bias: i32,
    pub single_bias: i32,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = solver_profile)]
pub struct NewSolverProfile {
    pub team_id: TeamId,
    pub name: String,
    pub description: Option<String>,
    pub skill_variance_weight: i32,
    pub pair_affinity_weight: i32,
    pub seat_affinity_weight: i32,
    pub side_preference_weight: i32,
    pub weight_class_slack_weight: i32,
    pub cox_cooldown_penalty: i32,
    pub placement_reward_weight: i32,
    pub pair_strength_weight: i32,
    pub bow_pair_strength_weight: i32,
    pub height_balance_weight: i32,
    pub end_pair_skill_weight: i32,
    pub engine_room_strength_weight: i32,
    pub partial_fill_bonus: i32,
    pub non_scull_retention_weight: i32,
    pub bow_cox_fit_weight: i32,
    pub top_boat_stacking_weight: i32,
    pub pair_eligibility_weight: i32,
    pub minimize_bench_weight: i32,
    pub boat_size_stacking_weight: i32,
    pub bench_cooldown_penalty: i32,
    pub stroke_spread_weight: i32,
    pub eight_bias: i32,
    pub coxed_four_bias: i32,
    pub four_bias: i32,
    pub quad_bias: i32,
    pub pair_bias: i32,
    pub double_bias: i32,
    pub single_bias: i32,
}

impl SolverProfile {
    /// List all profiles for a team, ordered by name.
    pub fn list_for_team(
        conn: &mut SqliteConnection,
        team_id: TeamId,
    ) -> Result<Vec<SolverProfile>, diesel::result::Error> {
        solver_profile::table
            .filter(solver_profile::team_id.eq(team_id))
            .order(solver_profile::name.asc())
            .select(SolverProfile::as_select())
            .get_results(conn)
    }

    /// Find a profile by (team, name).
    pub fn find_by_name(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        name: &str,
    ) -> Result<Option<SolverProfile>, diesel::result::Error> {
        solver_profile::table
            .filter(solver_profile::team_id.eq(team_id))
            .filter(solver_profile::name.eq(name))
            .select(SolverProfile::as_select())
            .first(conn)
            .optional()
    }

    /// Insert or update a profile by (team, name). On conflict,
    /// overwrites all weight columns.
    pub fn upsert(
        conn: &mut SqliteConnection,
        new: NewSolverProfile,
    ) -> Result<SolverProfile, diesel::result::Error> {
        // SQLite upsert: INSERT OR REPLACE (UNIQUE on team_id, name).
        diesel::replace_into(solver_profile::table)
            .values(&new)
            .returning(SolverProfile::as_returning())
            .get_result(conn)
    }

    /// Delete a profile by id.
    pub fn delete(
        conn: &mut SqliteConnection,
        id: SolverProfileId,
    ) -> Result<usize, diesel::result::Error> {
        diesel::delete(solver_profile::table.find(id)).execute(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team::{NewTeam, Team};
    use crate::test_support::in_memory_conn;

    fn seed_team(conn: &mut diesel::SqliteConnection) -> TeamId {
        let now = chrono::Utc::now().naive_utc();
        Team::create(
            conn,
            NewTeam {
                name: "Test".into(),
                created_at: now,
            },
        )
        .unwrap()
        .id
    }

    fn make_profile(tid: TeamId, name: &str) -> NewSolverProfile {
        NewSolverProfile {
            team_id: tid,
            name: name.into(),
            description: None,
            skill_variance_weight: 1,
            pair_affinity_weight: 1,
            seat_affinity_weight: 1,
            side_preference_weight: 1,
            weight_class_slack_weight: 1,
            cox_cooldown_penalty: 1,
            placement_reward_weight: 1,
            pair_strength_weight: 1,
            bow_pair_strength_weight: 1,
            height_balance_weight: 1,
            end_pair_skill_weight: 1,
            engine_room_strength_weight: 1,
            partial_fill_bonus: 1,
            non_scull_retention_weight: 1,
            bow_cox_fit_weight: 1,
            top_boat_stacking_weight: 1,
            pair_eligibility_weight: 1,
            minimize_bench_weight: 1,
            boat_size_stacking_weight: 1,
            bench_cooldown_penalty: 1,
            stroke_spread_weight: 1,
            eight_bias: 0,
            coxed_four_bias: 0,
            four_bias: 0,
            quad_bias: 0,
            pair_bias: 0,
            double_bias: 0,
            single_bias: 0,
        }
    }

    #[test]
    fn upsert_and_find_by_name() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);

        let p = SolverProfile::upsert(&mut conn, make_profile(tid, "Race Day")).unwrap();
        assert_eq!(p.name, "Race Day");

        let found = SolverProfile::find_by_name(&mut conn, tid, "Race Day")
            .unwrap()
            .unwrap();
        assert_eq!(found.id, p.id);
        assert!(SolverProfile::find_by_name(&mut conn, tid, "Nope")
            .unwrap()
            .is_none());
    }

    #[test]
    fn list_for_team() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);

        SolverProfile::upsert(&mut conn, make_profile(tid, "B Profile")).unwrap();
        SolverProfile::upsert(&mut conn, make_profile(tid, "A Profile")).unwrap();

        let list = SolverProfile::list_for_team(&mut conn, tid).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "A Profile"); // ordered by name
    }

    #[test]
    fn delete() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);
        let p = SolverProfile::upsert(&mut conn, make_profile(tid, "X")).unwrap();

        let deleted = SolverProfile::delete(&mut conn, p.id).unwrap();
        assert_eq!(deleted, 1);
        assert!(SolverProfile::find_by_name(&mut conn, tid, "X")
            .unwrap()
            .is_none());
    }

    #[test]
    fn upsert_replaces_on_same_name() {
        let mut conn = in_memory_conn();
        let tid = seed_team(&mut conn);

        let mut prof = make_profile(tid, "My Prof");
        prof.skill_variance_weight = 10;
        SolverProfile::upsert(&mut conn, prof).unwrap();

        let mut prof2 = make_profile(tid, "My Prof");
        prof2.skill_variance_weight = 99;
        SolverProfile::upsert(&mut conn, prof2).unwrap();

        let list = SolverProfile::list_for_team(&mut conn, tid).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].skill_variance_weight, 99);
    }
}
