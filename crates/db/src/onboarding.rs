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
