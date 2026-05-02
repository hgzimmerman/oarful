//! Auth identity for the tenant. Separate from `rower` so non-rowing
//! users (Program Directors) can have accounts. `app_user.rower_id`
//! optionally links to a rower profile.

use crate::rower::types::RowerId;
use crate::schema::{app_user, user_role};
use crate::types::IntBool;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, diesel_derive_enum::DbEnum)]
#[DbValueStyle = "snake_case"]
pub enum UserStatus {
    Invited,
    Active,
    Disabled,
}

impl std::fmt::Display for UserStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl UserStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Invited => "invited",
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }
}

impl std::str::FromStr for UserStatus {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "invited" => Ok(Self::Invited),
            "active" => Ok(Self::Active),
            "disabled" => Ok(Self::Disabled),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, diesel_derive_enum::DbEnum)]
#[DbValueStyle = "PascalCase"]
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

impl std::str::FromStr for Role {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Member" => Ok(Self::Member),
            "Coach" => Ok(Self::Coach),
            "ProgramDirector" => Ok(Self::ProgramDirector),
            _ => Err(()),
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
    pub status: UserStatus,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub opt_in_reminders: IntBool,
    pub opt_in_lineups: IntBool,
    pub rower_id: Option<RowerId>,
    pub opt_in_stale_alerts: IntBool,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

#[derive(Debug, Clone, diesel::Insertable)]
#[diesel(table_name = crate::schema::app_user)]
pub struct NewAppUser {
    pub email: String,
    pub password_hash: Option<String>,
    pub name: String,
    pub status: UserStatus,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

#[derive(Debug, Clone, diesel::Queryable, diesel::Selectable, diesel::Insertable)]
#[diesel(table_name = crate::schema::user_role)]
pub struct UserRoleRow {
    pub user_id: UserId,
    pub role: Role,
}

impl AppUser {
    /// Preferred display name. Uses first + last when available,
    /// falls back to the legacy `name` column for unsplit records.
    pub fn display_name(&self) -> String {
        match (self.first_name.as_deref(), self.last_name.as_deref()) {
            (Some(first), Some(last)) => format!("{first} {last}"),
            (Some(first), None) => first.to_string(),
            (None, Some(last)) => last.to_string(),
            (None, None) => self.name.clone(),
        }
    }

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
                app_user::status.eq(UserStatus::Active),
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
        Ok(row.map(|r| r.role))
    }

    pub fn set_role(
        conn: &mut SqliteConnection,
        user_id: UserId,
        role: Role,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(user_role::table)
            .values(UserRoleRow { user_id, role })
            .on_conflict(user_role::user_id)
            .do_update()
            .set(user_role::role.eq(role))
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
            .set(app_user::status.eq(status))
            .execute(conn)?;
        Ok(())
    }

    /// Whether this user has opted in to availability reminder emails.
    pub fn wants_reminders(&self) -> bool {
        self.opt_in_reminders.as_bool()
    }

    /// Whether this user has opted in to lineup notification emails.
    pub fn wants_lineups(&self) -> bool {
        self.opt_in_lineups.as_bool()
    }

    /// Whether this user has opted in to stale lineup alert emails.
    pub fn wants_stale_alerts(&self) -> bool {
        self.opt_in_stale_alerts.as_bool()
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
        opt_in_stale_alerts: bool,
    ) -> Result<(), diesel::result::Error> {
        let now = chrono::Utc::now().naive_utc();
        diesel::update(app_user::table.find(user_id))
            .set((
                app_user::opt_in_reminders.eq(IntBool::new(opt_in_reminders)),
                app_user::opt_in_lineups.eq(IntBool::new(opt_in_lineups)),
                app_user::opt_in_stale_alerts.eq(IntBool::new(opt_in_stale_alerts)),
                app_user::updated_at.eq(now),
            ))
            .execute(conn)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Role::at_least ──────────────────────────────────────────────

    #[test]
    fn role_hierarchy() {
        assert!(Role::ProgramDirector.at_least(Role::ProgramDirector));
        assert!(Role::ProgramDirector.at_least(Role::Coach));
        assert!(Role::ProgramDirector.at_least(Role::Member));

        assert!(!Role::Coach.at_least(Role::ProgramDirector));
        assert!(Role::Coach.at_least(Role::Coach));
        assert!(Role::Coach.at_least(Role::Member));

        assert!(!Role::Member.at_least(Role::ProgramDirector));
        assert!(!Role::Member.at_least(Role::Coach));
        assert!(Role::Member.at_least(Role::Member));
    }

    #[test]
    fn role_from_str_round_trip() {
        for role in [Role::Member, Role::Coach, Role::ProgramDirector] {
            let s = role.as_str();
            assert_eq!(s.parse::<Role>(), Ok(role));
        }
    }

    #[test]
    fn role_from_str_unknown() {
        assert!("Admin".parse::<Role>().is_err());
        assert!("".parse::<Role>().is_err());
    }

    // ── UserStatus ──────────────────────────────────────────────────

    #[test]
    fn user_status_round_trip() {
        for status in [
            UserStatus::Invited,
            UserStatus::Active,
            UserStatus::Disabled,
        ] {
            let s = status.as_str();
            assert_eq!(s.parse::<UserStatus>(), Ok(status));
        }
    }

    #[test]
    fn user_status_from_str_unknown() {
        assert!("banned".parse::<UserStatus>().is_err());
    }

    #[test]
    fn user_status_display() {
        assert_eq!(UserStatus::Active.to_string(), "active");
        assert_eq!(UserStatus::Invited.to_string(), "invited");
        assert_eq!(UserStatus::Disabled.to_string(), "disabled");
    }

    // ── UserId ──────────────────────────────────────────────────────

    #[test]
    fn user_id_round_trip() {
        let id = UserId::new(99);
        assert_eq!(id.as_int(), 99);
        assert_eq!(id.to_string(), "99");
    }

    // ── DB-dependent tests ──────────────────────────────────────────

    use crate::test_support::in_memory_conn;

    fn seed_user(conn: &mut diesel::SqliteConnection) -> AppUser {
        let now = chrono::Utc::now().naive_utc();
        AppUser::create(
            conn,
            NewAppUser {
                email: "test@example.com".into(),
                password_hash: None,
                name: "Test User".into(),
                first_name: None,
                last_name: None,
                status: UserStatus::Invited,
                created_at: now,
                updated_at: now,
            },
        )
        .expect("seed user")
    }

    #[test]
    fn create_and_get() {
        let mut conn = in_memory_conn();
        let user = seed_user(&mut conn);
        let fetched = AppUser::get(&mut conn, user.id).unwrap().unwrap();
        assert_eq!(fetched.email, "test@example.com");
        assert_eq!(fetched.status, UserStatus::Invited);
    }

    #[test]
    fn find_by_email() {
        let mut conn = in_memory_conn();
        seed_user(&mut conn);
        let found = AppUser::find_by_email(&mut conn, "test@example.com")
            .unwrap()
            .unwrap();
        assert_eq!(found.name, "Test User");
        assert!(AppUser::find_by_email(&mut conn, "nope@example.com")
            .unwrap()
            .is_none());
    }

    #[test]
    fn set_role_and_read() {
        let mut conn = in_memory_conn();
        let user = seed_user(&mut conn);
        assert!(AppUser::role(&mut conn, user.id).unwrap().is_none());

        AppUser::set_role(&mut conn, user.id, Role::Coach).unwrap();
        assert_eq!(
            AppUser::role(&mut conn, user.id).unwrap(),
            Some(Role::Coach)
        );

        // Upsert to PD
        AppUser::set_role(&mut conn, user.id, Role::ProgramDirector).unwrap();
        assert_eq!(
            AppUser::role(&mut conn, user.id).unwrap(),
            Some(Role::ProgramDirector)
        );
    }

    #[test]
    fn set_status() {
        let mut conn = in_memory_conn();
        let user = seed_user(&mut conn);
        assert_eq!(user.status, UserStatus::Invited);

        AppUser::set_status(&mut conn, user.id, UserStatus::Active).unwrap();
        let fetched = AppUser::get(&mut conn, user.id).unwrap().unwrap();
        assert_eq!(fetched.status, UserStatus::Active);
    }

    #[test]
    fn set_password_and_activate() {
        let mut conn = in_memory_conn();
        let user = seed_user(&mut conn);
        AppUser::set_password_and_activate(&mut conn, user.id, "hashed").unwrap();
        let fetched = AppUser::get(&mut conn, user.id).unwrap().unwrap();
        assert_eq!(fetched.status, UserStatus::Active);
        assert_eq!(fetched.password_hash.as_deref(), Some("hashed"));
    }

    #[test]
    fn email_prefs() {
        let mut conn = in_memory_conn();
        let user = seed_user(&mut conn);
        // Defaults are opt-in (1) per migration
        assert!(user.wants_reminders());
        assert!(user.wants_lineups());
        assert!(user.wants_stale_alerts());

        AppUser::set_email_prefs(&mut conn, user.id, false, false, false).unwrap();
        let fetched = AppUser::get(&mut conn, user.id).unwrap().unwrap();
        assert!(!fetched.wants_reminders());
        assert!(!fetched.wants_lineups());
        assert!(!fetched.wants_stale_alerts());
    }
}
