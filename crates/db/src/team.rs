//! Team: a named roster + practice schedule that shares the tenant's
//! fleet. Rowers and coaches can belong to multiple teams; the solver
//! runs per (team, date).

use crate::boat::types::BoatId;
use crate::rower::types::RowerId;
use crate::schema::{team, team_boat_default, team_membership};
use crate::types::{DurationMinutes, IntBool};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use diesel::prelude::*;
use diesel::SqliteConnection;
use serde::{Deserialize, Serialize};

// =====================================================================
// PracticeDays bitmask newtype
// =====================================================================

/// Bitmask of weekdays a team typically practices on.
/// Bit 0 = Monday, bit 1 = Tuesday, ..., bit 6 = Sunday.
/// Stored as a nullable integer column; `None` means "not configured".
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    diesel_derive_newtype::DieselNewType,
)]
pub struct PracticeDays(i32);

impl PracticeDays {
    pub const EMPTY: Self = Self(0);

    pub fn from_weekdays(days: &[Weekday]) -> Self {
        let mut bits = 0i32;
        for d in days {
            bits |= 1 << d.num_days_from_monday();
        }
        Self(bits)
    }

    pub fn contains(self, day: Weekday) -> bool {
        self.0 & (1 << day.num_days_from_monday()) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Iterate over the weekdays that are set, Monday first.
    pub fn weekdays(self) -> impl Iterator<Item = Weekday> {
        const ALL: [Weekday; 7] = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ];
        let bits = self.0;
        ALL.into_iter()
            .filter(move |d| bits & (1 << d.num_days_from_monday()) != 0)
    }

    /// Find the next date on or after `from` that matches one of the
    /// configured weekdays and is NOT in `existing`. Returns `None` if
    /// no days are configured or nothing is found within 60 days.
    pub fn next_unfilled(
        self,
        from: NaiveDate,
        existing: &std::collections::HashSet<NaiveDate>,
    ) -> Option<NaiveDate> {
        if self.is_empty() {
            return None;
        }
        let mut candidate = from;
        for _ in 0..60 {
            if self.contains(candidate.weekday()) && !existing.contains(&candidate) {
                return Some(candidate);
            }
            candidate = candidate.succ_opt()?;
        }
        None
    }
}

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
    pub self_edit_level: SelfEditLevel,
    /// Default time of day for new practices. None = not set.
    pub default_practice_time: Option<NaiveTime>,
    /// Default practice duration in minutes. None = not set.
    pub default_practice_duration_minutes: Option<DurationMinutes>,
    /// Soft-delete flag. Archived teams are hidden from operational
    /// views but preserved for historical lineups.
    pub archived: IntBool,
    /// Bitmask of default practice weekdays (Mon=bit0 … Sun=bit6).
    pub default_practice_days: Option<PracticeDays>,
    /// When true, rowers who haven't responded are treated as available.
    pub assume_available: IntBool,
    /// Which erg test distance (metres) the team uses for strength
    /// bucketing. None = not configured.
    pub erg_threshold_distance_m: Option<i32>,
}

/// What a non-coach member is allowed to edit on their own profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, diesel_derive_enum::DbEnum)]
#[DbValueStyle = "snake_case"]
#[serde(rename_all = "snake_case")]
pub enum SelfEditLevel {
    /// Side, designated cox, can scull only.
    Low,
    /// Low + height.
    Medium,
    /// All attributes except active.
    High,
}

impl std::fmt::Display for SelfEditLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl SelfEditLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

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
    Debug, Clone, PartialEq, Eq, diesel::Queryable, diesel::Selectable, diesel::Insertable,
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

    /// Non-archived teams, ordered by name. Used by operational views
    /// (team switcher, sync, practices). PD admin views use `list_all`.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn list_active(conn: &mut SqliteConnection) -> Result<Vec<Team>, diesel::result::Error> {
        team::table
            .filter(team::archived.eq(0))
            .select(Team::as_select())
            .order(team::name.asc())
            .get_results(conn)
    }

    /// Toggle the archived flag. PD only.
    #[tracing::instrument(level = "debug", skip(conn), err)]
    pub fn set_archived(
        conn: &mut SqliteConnection,
        id: TeamId,
        archived: bool,
    ) -> Result<(), diesel::result::Error> {
        diesel::update(team::table.find(id))
            .set(team::archived.eq(if archived { 1 } else { 0 }))
            .execute(conn)?;
        Ok(())
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

    /// Best-effort default team for a user: look up their linked rower's
    /// team membership first, then fall back to `Team::first`.
    pub fn default_for_user(
        conn: &mut SqliteConnection,
        user_id: crate::app_user::UserId,
    ) -> Result<Option<Team>, diesel::result::Error> {
        use crate::app_user::AppUser;
        if let Some(user) = AppUser::get(conn, user_id)? {
            if let Some(rower_id) = user.rower_id {
                let team_ids = TeamMembership::team_ids_for_rower(conn, rower_id)?;
                if let Some(tid) = team_ids.first() {
                    if let Some(t) = team::table
                        .find(*tid)
                        .select(Team::as_select())
                        .first(conn)
                        .optional()?
                    {
                        return Ok(Some(t));
                    }
                }
            }
        }
        Self::first(conn)
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
        user_id: crate::app_user::UserId,
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
    pub fn all(conn: &mut SqliteConnection) -> Result<Vec<TeamMembership>, diesel::result::Error> {
        team_membership::table
            .select(TeamMembership::as_select())
            .get_results(conn)
    }
}

// =====================================================================
// Per-team default boat selection
// =====================================================================

#[derive(
    Debug, Clone, PartialEq, Eq, diesel::Queryable, diesel::Selectable, diesel::Insertable,
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
    pub fn all(conn: &mut SqliteConnection) -> Result<Vec<TeamBoatDefault>, diesel::result::Error> {
        team_boat_default::table
            .select(TeamBoatDefault::as_select())
            .get_results(conn)
    }
}
