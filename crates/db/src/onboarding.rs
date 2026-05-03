//! Per-user onboarding progress tracking.

use std::collections::HashSet;

use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::app_user::UserId;
use crate::schema::onboarding_progress;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnboardingStep {
    AddBoats,
    AddRowers,
    CustomizeRower,
    CreatePractice,
    GenerateLineup,
    Dismissed,
}

impl OnboardingStep {
    fn as_str(self) -> &'static str {
        match self {
            Self::AddBoats => "add_boats",
            Self::AddRowers => "add_rowers",
            Self::CustomizeRower => "customize_rower",
            Self::CreatePractice => "create_practice",
            Self::GenerateLineup => "generate_lineup",
            Self::Dismissed => "dismissed",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "add_boats" => Some(Self::AddBoats),
            "add_rowers" => Some(Self::AddRowers),
            "customize_rower" => Some(Self::CustomizeRower),
            "create_practice" => Some(Self::CreatePractice),
            "generate_lineup" => Some(Self::GenerateLineup),
            "dismissed" => Some(Self::Dismissed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = onboarding_progress)]
struct OnboardingRow {
    #[allow(dead_code)]
    app_user_id: i32,
    step: String,
    #[allow(dead_code)]
    completed_at: NaiveDateTime,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = onboarding_progress)]
struct NewOnboardingRow {
    app_user_id: i32,
    step: String,
}

/// Record that a user completed an onboarding step. Idempotent —
/// silently ignores duplicates.
pub fn complete_step(
    conn: &mut SqliteConnection,
    user_id: UserId,
    step: OnboardingStep,
) -> Result<(), diesel::result::Error> {
    diesel::insert_or_ignore_into(onboarding_progress::table)
        .values(NewOnboardingRow {
            app_user_id: user_id.as_int(),
            step: step.as_str().to_string(),
        })
        .execute(conn)?;
    Ok(())
}

/// Fetch all completed onboarding steps for a user.
pub fn completed_steps(
    conn: &mut SqliteConnection,
    user_id: UserId,
) -> Result<HashSet<OnboardingStep>, diesel::result::Error> {
    let rows: Vec<OnboardingRow> = onboarding_progress::table
        .filter(onboarding_progress::app_user_id.eq(user_id.as_int()))
        .select(OnboardingRow::as_select())
        .load(conn)?;
    Ok(rows
        .iter()
        .filter_map(|r| OnboardingStep::from_str(&r.step))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_user::{AppUser, NewAppUser, UserStatus};
    use crate::test_support::in_memory_conn;

    /// Seed an app_user row so FK constraints pass.
    fn seed_user(conn: &mut diesel::SqliteConnection, id: i32) -> UserId {
        let now = chrono::Utc::now().naive_utc();
        let user = AppUser::create(
            conn,
            NewAppUser {
                email: format!("user{id}@test.com"),
                password_hash: None,
                name: format!("User {id}"),
                first_name: None,
                last_name: None,
                status: UserStatus::Active,
                created_at: now,
                updated_at: now,
            },
        )
        .unwrap();
        user.id
    }

    /// Every variant survives an as_str → from_str round trip.
    #[test]
    fn as_str_from_str_round_trip() {
        let all = [
            OnboardingStep::AddBoats,
            OnboardingStep::AddRowers,
            OnboardingStep::CustomizeRower,
            OnboardingStep::CreatePractice,
            OnboardingStep::GenerateLineup,
            OnboardingStep::Dismissed,
        ];
        for step in all {
            let s = step.as_str();
            let back =
                OnboardingStep::from_str(s).unwrap_or_else(|| panic!("from_str failed for {s:?}"));
            assert_eq!(back, step, "round-trip mismatch for {s:?}");
        }
    }

    /// Unknown strings yield None (forward compat).
    #[test]
    fn from_str_unknown_returns_none() {
        assert!(OnboardingStep::from_str("nonexistent").is_none());
        assert!(OnboardingStep::from_str("").is_none());
    }

    /// complete_step inserts and completed_steps retrieves.
    #[test]
    fn complete_and_read_steps() {
        let mut conn = in_memory_conn();
        let uid = seed_user(&mut conn, 1);

        assert!(completed_steps(&mut conn, uid).unwrap().is_empty());

        complete_step(&mut conn, uid, OnboardingStep::AddBoats).unwrap();
        complete_step(&mut conn, uid, OnboardingStep::AddRowers).unwrap();

        let steps = completed_steps(&mut conn, uid).unwrap();
        assert_eq!(steps.len(), 2);
        assert!(steps.contains(&OnboardingStep::AddBoats));
        assert!(steps.contains(&OnboardingStep::AddRowers));
    }

    /// Duplicate inserts are silently ignored (INSERT OR IGNORE).
    #[test]
    fn complete_step_is_idempotent() {
        let mut conn = in_memory_conn();
        let uid = seed_user(&mut conn, 1);

        complete_step(&mut conn, uid, OnboardingStep::CreatePractice).unwrap();
        complete_step(&mut conn, uid, OnboardingStep::CreatePractice).unwrap();

        let steps = completed_steps(&mut conn, uid).unwrap();
        assert_eq!(steps.len(), 1);
    }

    /// Steps are per-user — user 2 doesn't see user 1's steps.
    #[test]
    fn steps_are_per_user() {
        let mut conn = in_memory_conn();
        let u1 = seed_user(&mut conn, 1);
        let u2 = seed_user(&mut conn, 2);

        complete_step(&mut conn, u1, OnboardingStep::AddBoats).unwrap();
        complete_step(&mut conn, u2, OnboardingStep::GenerateLineup).unwrap();

        let s1 = completed_steps(&mut conn, u1).unwrap();
        let s2 = completed_steps(&mut conn, u2).unwrap();
        assert_eq!(s1.len(), 1);
        assert!(s1.contains(&OnboardingStep::AddBoats));
        assert_eq!(s2.len(), 1);
        assert!(s2.contains(&OnboardingStep::GenerateLineup));
    }
}
