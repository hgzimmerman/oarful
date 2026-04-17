//! Auth identity for the tenant. Separate from `rower` so non-rowing
//! users (Program Directors) can have accounts. `app_user.rower_id`
//! optionally links to a rower profile.

use crate::rower::types::RowerId;
use crate::schema::{app_user, user_role};
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::SqliteConnection;
use serde::{Deserialize, Serialize};

/// Newtyped user ID within a tenant DB.
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
pub struct UserId(i32);

impl UserId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserStatus {
    Invited,
    Active,
    Disabled,
}

impl UserStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Invited => "invited",
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "invited" => Some(Self::Invited),
            "active" => Some(Self::Active),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Member,
    Coach,
    ProgramDirector,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Member => "Member",
            Self::Coach => "Coach",
            Self::ProgramDirector => "ProgramDirector",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Member" => Some(Self::Member),
            "Coach" => Some(Self::Coach),
            "ProgramDirector" => Some(Self::ProgramDirector),
            _ => None,
        }
    }

    /// Returns true if `self` is at least as privileged as `min`.
    pub fn at_least(&self, min: Role) -> bool {
        self.ordinal() >= min.ordinal()
    }

    fn ordinal(&self) -> u8 {
        match self {
            Self::Member => 0,
            Self::Coach => 1,
            Self::ProgramDirector => 2,
        }
    }
}

#[derive(Debug, Clone, diesel::Queryable, diesel::Selectable)]
#[diesel(table_name = crate::schema::app_user)]
pub struct AppUser {
    pub id: UserId,
    pub email: String,
    pub password_hash: Option<String>,
    pub name: String,
    pub status: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub opt_in_reminders: i32,
    pub opt_in_lineups: i32,
    pub rower_id: Option<RowerId>,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::app_user)]
pub struct NewAppUser {
    pub email: String,
    pub password_hash: Option<String>,
    pub name: String,
    pub status: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, diesel::Queryable, diesel::Selectable, diesel::Insertable)]
#[diesel(table_name = crate::schema::user_role)]
pub struct UserRoleRow {
    pub user_id: UserId,
    pub role: String,
}

impl AppUser {
    pub fn find_by_email(
        conn: &mut SqliteConnection,
        email_addr: &str,
    ) -> Result<Option<AppUser>, diesel::result::Error> {
        app_user::table
            .filter(app_user::email.eq(email_addr))
            .select(AppUser::as_select())
            .first(conn)
            .optional()
    }

    pub fn get(
        conn: &mut SqliteConnection,
        id: UserId,
    ) -> Result<Option<AppUser>, diesel::result::Error> {
        app_user::table
            .find(id)
            .select(AppUser::as_select())
            .first(conn)
            .optional()
    }

    pub fn create(
        conn: &mut SqliteConnection,
        new: NewAppUser,
    ) -> Result<AppUser, diesel::result::Error> {
        diesel::insert_into(app_user::table)
            .values(new)
            .returning(AppUser::as_returning())
            .get_result(conn)
    }

    pub fn set_password_and_activate(
        conn: &mut SqliteConnection,
        id: UserId,
        hash: &str,
    ) -> Result<(), diesel::result::Error> {
        let now = chrono::Utc::now().naive_utc();
        diesel::update(app_user::table.find(id))
            .set((
                app_user::password_hash.eq(hash),
                app_user::status.eq("active"),
                app_user::updated_at.eq(now),
            ))
            .execute(conn)?;
        Ok(())
    }

    /// Update only the password hash (user is already active).
    pub fn set_password(
        conn: &mut SqliteConnection,
        id: UserId,
        hash: &str,
    ) -> Result<(), diesel::result::Error> {
        let now = chrono::Utc::now().naive_utc();
        diesel::update(app_user::table.find(id))
            .set((
                app_user::password_hash.eq(hash),
                app_user::updated_at.eq(now),
            ))
            .execute(conn)?;
        Ok(())
    }

    pub fn role(
        conn: &mut SqliteConnection,
        user_id: UserId,
    ) -> Result<Option<Role>, diesel::result::Error> {
        let row: Option<UserRoleRow> = user_role::table
            .find(user_id)
            .select(UserRoleRow::as_select())
            .first(conn)
            .optional()?;
        Ok(row.and_then(|r| Role::from_str(&r.role)))
    }

    pub fn set_role(
        conn: &mut SqliteConnection,
        user_id: UserId,
        role: Role,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(user_role::table)
            .values(UserRoleRow {
                user_id,
                role: role.as_str().to_string(),
            })
            .on_conflict(user_role::user_id)
            .do_update()
            .set(user_role::role.eq(role.as_str()))
            .execute(conn)?;
        Ok(())
    }

    pub fn set_status(
        conn: &mut SqliteConnection,
        user_id: UserId,
        status: UserStatus,
    ) -> Result<(), diesel::result::Error> {
        use crate::schema::app_user;
        diesel::update(app_user::table.find(user_id))
            .set(app_user::status.eq(status.as_str()))
            .execute(conn)?;
        Ok(())
    }

    pub fn parsed_status(&self) -> Option<UserStatus> {
        UserStatus::from_str(&self.status)
    }

    /// Whether this user has opted in to availability reminder emails.
    pub fn wants_reminders(&self) -> bool {
        self.opt_in_reminders != 0
    }

    /// Whether this user has opted in to lineup notification emails.
    pub fn wants_lineups(&self) -> bool {
        self.opt_in_lineups != 0
    }

    /// Link this user to a rower profile.
    pub fn set_rower_id(
        conn: &mut SqliteConnection,
        user_id: UserId,
        rower_id: Option<RowerId>,
    ) -> Result<(), diesel::result::Error> {
        let now = chrono::Utc::now().naive_utc();
        diesel::update(app_user::table.find(user_id))
            .set((
                app_user::rower_id.eq(rower_id),
                app_user::updated_at.eq(now),
            ))
            .execute(conn)?;
        Ok(())
    }

    /// Find the user linked to a rower. Returns None if no user is
    /// linked to this rower.
    pub fn find_by_rower_id(
        conn: &mut SqliteConnection,
        rower_id: RowerId,
    ) -> Result<Option<AppUser>, diesel::result::Error> {
        app_user::table
            .filter(app_user::rower_id.eq(rower_id))
            .select(AppUser::as_select())
            .first(conn)
            .optional()
    }

    /// Update email opt-in preferences.
    pub fn set_email_prefs(
        conn: &mut SqliteConnection,
        user_id: UserId,
        opt_in_reminders: bool,
        opt_in_lineups: bool,
    ) -> Result<(), diesel::result::Error> {
        let now = chrono::Utc::now().naive_utc();
        diesel::update(app_user::table.find(user_id))
            .set((
                app_user::opt_in_reminders.eq(if opt_in_reminders { 1 } else { 0 }),
                app_user::opt_in_lineups.eq(if opt_in_lineups { 1 } else { 0 }),
                app_user::updated_at.eq(now),
            ))
            .execute(conn)?;
        Ok(())
    }
}
