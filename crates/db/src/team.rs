//! Team: a named roster + practice schedule that shares the tenant's
//! fleet. Rowers and coaches can belong to multiple teams; the solver
//! runs per (team, date).

use crate::rower::types::RowerId;
use crate::schema::{team, team_membership};
use chrono::NaiveDateTime;
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
}
