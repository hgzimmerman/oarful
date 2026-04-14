//! Team: a named roster + practice schedule that shares the tenant's
//! fleet. Rowers and coaches can belong to multiple teams; the solver
//! runs per (team, date).

use crate::boat::types::BoatId;
use crate::rower::types::RowerId;
use crate::schema::{team, team_boat_default, team_membership};
use chrono::{NaiveDateTime, NaiveTime};
use diesel::prelude::*;
use diesel::SqliteConnection;
use serde::{Deserialize, Serialize};

/// Newtyped identifier for a `team` row.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct TeamId(i32);

impl TeamId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for TeamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for TeamId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Identifiable,
)]
#[diesel(table_name = crate::schema::team)]
pub struct Team {
    pub id: TeamId,
    pub name: String,
    pub created_at: NaiveDateTime,
    /// Controls what members can self-edit on their profile.
    /// "low" | "medium" | "high". Default "low".
    pub self_edit_level: String,
    /// Default time of day for new practices. None = not set.
    pub default_practice_time: Option<NaiveTime>,
}

/// What a non-coach member is allowed to edit on their own profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfEditLevel {
    /// Side, designated cox, can scull only.
    Low,
    /// Low + height.
    Medium,
    /// All attributes except active.
    High,
}

impl SelfEditLevel {
    pub fn from_str(s: &str) -> Self {
        match s {
            "high" => Self::High,
            "medium" => Self::Medium,
            _ => Self::Low,
        }
    }

    pub fn can_edit_weight_class(self) -> bool {
        self == Self::High
    }
    pub fn can_edit_skill(self) -> bool {
        self == Self::High
    }
    pub fn can_edit_strength(self) -> bool {
        self == Self::High
    }
    pub fn can_edit_height(self) -> bool {
        matches!(self, Self::Medium | Self::High)
    }
    // Side, side_strength, can_scull, designated_cox, can_cox: always editable
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::team)]
pub struct NewTeam {
    pub name: String,
    pub created_at: NaiveDateTime,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Insertable,
)]
#[diesel(table_name = crate::schema::team_membership)]
pub struct TeamMembership {
    pub team_id: TeamId,
    pub rower_id: RowerId,
}

impl Team {
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_all(conn: &mut SqliteConnection) -> Result<Vec<Team>, diesel::result::Error> {
        team::table
            .select(Team::as_select())
            .order(team::name.asc())
            .get_results(conn)
    }

    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn get(
        conn: &mut SqliteConnection,
        id: TeamId,
    ) -> Result<Option<Team>, diesel::result::Error> {
        team::table
            .find(id)
            .select(Team::as_select())
            .first(conn)
            .optional()
    }

    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn create(
        conn: &mut SqliteConnection,
        new: NewTeam,
    ) -> Result<Team, diesel::result::Error> {
        diesel::insert_into(team::table)
            .values(new)
            .returning(Team::as_returning())
            .get_result(conn)
    }

    /// The first team in the DB, used as a default when no team is
    /// explicitly selected (e.g. the CLI without `--team`).
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn first(conn: &mut SqliteConnection) -> Result<Option<Team>, diesel::result::Error> {
        team::table
            .select(Team::as_select())
            .order(team::id.asc())
            .first(conn)
            .optional()
    }
}

impl TeamMembership {
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn add(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        rower_id: RowerId,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_or_ignore_into(team_membership::table)
            .values(TeamMembership { team_id, rower_id })
            .execute(conn)?;
        Ok(())
    }

    /// All team IDs a coach (user) is assigned to.
    pub fn team_ids_for_coach(
        conn: &mut SqliteConnection,
        user_id: i32,
    ) -> Result<Vec<TeamId>, diesel::result::Error> {
        use crate::schema::team_coach;
        team_coach::table
            .filter(team_coach::user_id.eq(user_id))
            .select(team_coach::team_id)
            .get_results(conn)
    }

    /// All team IDs a rower belongs to.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn team_ids_for_rower(
        conn: &mut SqliteConnection,
        rower_id: RowerId,
    ) -> Result<Vec<TeamId>, diesel::result::Error> {
        team_membership::table
            .filter(team_membership::rower_id.eq(rower_id))
            .select(team_membership::team_id)
            .get_results(conn)
    }

    /// All rower IDs on a team. Used to scope DbSnapshot rower lists.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn rower_ids_for_team(
        conn: &mut SqliteConnection,
        team_id: TeamId,
    ) -> Result<Vec<RowerId>, diesel::result::Error> {
        team_membership::table
            .filter(team_membership::team_id.eq(team_id))
            .select(team_membership::rower_id)
            .get_results(conn)
    }

    /// Remove a rower from a team.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn remove(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        rower_id: RowerId,
    ) -> Result<(), diesel::result::Error> {
        diesel::delete(
            team_membership::table
                .filter(team_membership::team_id.eq(team_id))
                .filter(team_membership::rower_id.eq(rower_id)),
        )
        .execute(conn)?;
        Ok(())
    }

    /// All (team_id, rower_id) pairs across the tenant.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn all(
        conn: &mut SqliteConnection,
    ) -> Result<Vec<TeamMembership>, diesel::result::Error> {
        team_membership::table
            .select(TeamMembership::as_select())
            .get_results(conn)
    }
}

// =====================================================================
// Per-team default boat selection
// =====================================================================

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Insertable,
)]
#[diesel(table_name = crate::schema::team_boat_default)]
pub struct TeamBoatDefault {
    pub team_id: TeamId,
    pub boat_id: BoatId,
}

impl TeamBoatDefault {
    /// Add a boat to a team's default set.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn add(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        boat_id: BoatId,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_or_ignore_into(team_boat_default::table)
            .values(TeamBoatDefault { team_id, boat_id })
            .execute(conn)?;
        Ok(())
    }

    /// Remove a boat from a team's default set.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn remove(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        boat_id: BoatId,
    ) -> Result<(), diesel::result::Error> {
        diesel::delete(
            team_boat_default::table
                .filter(team_boat_default::team_id.eq(team_id))
                .filter(team_boat_default::boat_id.eq(boat_id)),
        )
        .execute(conn)?;
        Ok(())
    }

    /// Default boat IDs for a team. Returns empty vec if none configured.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn boat_ids_for_team(
        conn: &mut SqliteConnection,
        team_id: TeamId,
    ) -> Result<Vec<BoatId>, diesel::result::Error> {
        team_boat_default::table
            .filter(team_boat_default::team_id.eq(team_id))
            .select(team_boat_default::boat_id)
            .get_results(conn)
    }

    /// All (team_id, boat_id) pairs across the tenant.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn all(
        conn: &mut SqliteConnection,
    ) -> Result<Vec<TeamBoatDefault>, diesel::result::Error> {
        team_boat_default::table
            .select(TeamBoatDefault::as_select())
            .get_results(conn)
    }
}
