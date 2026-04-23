//! Rate-limiting log for sent emails. Tracks (team, type, date) to
//! prevent duplicate sends within the same day.

use crate::app_user::UserId;
use crate::schema::email_log;
use crate::team::TeamId;
use crate::types::EmailLogType;
use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use diesel::SqliteConnection;
use serde::{Deserialize, Serialize};

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
pub struct EmailLogId(i32);

impl EmailLogId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    pub fn as_int(&self) -> i32 {
        self.0
    }
}

impl std::fmt::Display for EmailLogId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for EmailLogId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        i32::from_str(s).map(Self)
    }
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = email_log)]
pub struct EmailLog {
    pub id: EmailLogId,
    pub team_id: TeamId,
    pub email_type: EmailLogType,
    pub practice_date: NaiveDate,
    pub sent_at: NaiveDateTime,
    pub recipient_count: i32,
    pub sent_by_user_id: UserId,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = email_log)]
pub struct NewEmailLog {
    pub team_id: TeamId,
    pub email_type: EmailLogType,
    pub practice_date: NaiveDate,
    pub sent_at: NaiveDateTime,
    pub recipient_count: i32,
    pub sent_by_user_id: UserId,
}

impl EmailLog {
    /// Check whether an email of this type was already sent for a
    /// practice date today. Returns true if a send already happened.
    pub fn already_sent_today(
        conn: &mut SqliteConnection,
        team_id: TeamId,
        email_type_val: &EmailLogType,
        practice_date: NaiveDate,
    ) -> Result<bool, diesel::result::Error> {
        let today = chrono::Utc::now().date_naive();
        let start_of_day = today.and_hms_opt(0, 0, 0).unwrap();
        let end_of_day = today.and_hms_opt(23, 59, 59).unwrap();

        let count: i64 = email_log::table
            .filter(email_log::team_id.eq(team_id))
            .filter(email_log::email_type.eq(email_type_val))
            .filter(email_log::practice_date.eq(practice_date))
            .filter(email_log::sent_at.ge(start_of_day))
            .filter(email_log::sent_at.le(end_of_day))
            .count()
            .get_result(conn)?;

        Ok(count > 0)
    }

    /// Record a sent email for rate-limiting purposes.
    pub fn record(
        conn: &mut SqliteConnection,
        entry: NewEmailLog,
    ) -> Result<(), diesel::result::Error> {
        diesel::insert_into(email_log::table)
            .values(&entry)
            .execute(conn)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::in_memory_conn;

    fn seed_team(conn: &mut diesel::SqliteConnection) -> TeamId {
        use crate::team::{NewTeam, Team};
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

    fn seed_user(conn: &mut diesel::SqliteConnection) -> UserId {
        use crate::app_user::{AppUser, NewAppUser, UserStatus};
        let now = chrono::Utc::now().naive_utc();
        AppUser::create(
            conn,
            NewAppUser {
                email: "test@example.com".into(),
                password_hash: None,
                name: "Test".into(),
                status: UserStatus::Active,
                created_at: now,
                updated_at: now,
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn record_and_already_sent_today() {
        let mut conn = in_memory_conn();
        let team_id = seed_team(&mut conn);
        let user_id = seed_user(&mut conn);
        let practice_date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let email_type = EmailLogType::new("reminder");

        // Not sent yet
        assert!(
            !EmailLog::already_sent_today(&mut conn, team_id, &email_type, practice_date).unwrap()
        );

        // Record a send with now as sent_at
        EmailLog::record(
            &mut conn,
            NewEmailLog {
                team_id,
                email_type: email_type.clone(),
                practice_date,
                sent_at: chrono::Utc::now().naive_utc(),
                recipient_count: 5,
                sent_by_user_id: user_id,
            },
        )
        .unwrap();

        // Now it should be detected as sent today
        assert!(
            EmailLog::already_sent_today(&mut conn, team_id, &email_type, practice_date).unwrap()
        );
    }

    #[test]
    fn already_sent_different_type_not_blocked() {
        let mut conn = in_memory_conn();
        let team_id = seed_team(&mut conn);
        let user_id = seed_user(&mut conn);
        let practice_date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();

        EmailLog::record(
            &mut conn,
            NewEmailLog {
                team_id,
                email_type: EmailLogType::new("reminder"),
                practice_date,
                sent_at: chrono::Utc::now().naive_utc(),
                recipient_count: 3,
                sent_by_user_id: user_id,
            },
        )
        .unwrap();

        // Different type should not be blocked
        let lineup_type = EmailLogType::new("lineup");
        assert!(
            !EmailLog::already_sent_today(&mut conn, team_id, &lineup_type, practice_date).unwrap()
        );
    }
}
