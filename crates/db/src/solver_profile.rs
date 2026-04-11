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
