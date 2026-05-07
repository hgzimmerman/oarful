//! Practice dashboard handlers: create, cancel, unified list.
//!
//! Email-related handlers live in submodules:
//! - [`reminders`] — availability reminder preview + send
//! - [`lineups`] — lineup notification preview + send

mod lineups;
mod reminders;

pub(crate) use lineups::{lineup_preview_handler, send_lineups_handler};
pub(crate) use reminders::{reminder_preview_handler, send_reminders_handler};

use axum::{
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::{NaiveDate, NaiveTime, Utc};
use lineup_db::app_user::Role;
use lineup_db::practice::{Practice, PracticeId};
use serde::Deserialize;
use std::collections::HashSet;

use crate::state::TenantContext;
use crate::{
    handlers::{bad_request, internal_error, ErrorResponse},
    templates,
};

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn list_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, ErrorResponse> {
    let is_coach = tenant.claims.role().at_least(Role::Coach);
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let onboarding = if is_coach {
        Some(query_onboarding_state(&tenant).await?)
    } else {
        None
    };

    let now = Utc::now().naive_utc();
    let today = now.date();
    // Show history going back 14 days.
    let history_since = today - chrono::Duration::days(14);

    let (practices, default_time, default_duration, suggested_date) = tenant
        .db
        .with_conn(move |conn| {
            let team = lineup_db::team::Team::get(conn, team_id)?;
            let default_time = team.as_ref().and_then(|t| t.default_practice_time);
            let default_duration = team
                .as_ref()
                .and_then(|t| t.default_practice_duration_minutes);
            let practice_days = team.as_ref().and_then(|t| t.default_practice_days);

            let practices =
                lineup_db::practice::list_with_phases(conn, team_id, now, history_since)?;

            let existing_dates: HashSet<chrono::NaiveDate> =
                practices.iter().map(|p| p.practice.date).collect();
            let suggested_date =
                practice_days.and_then(|pd| pd.next_unfilled(today, &existing_dates));

            Ok((practices, default_time, default_duration, suggested_date))
        })
        .await
        .map_err(internal_error)?;

    let content = templates::practices::unified_page(
        &practices,
        is_coach,
        today,
        default_time,
        default_duration,
        suggested_date,
        onboarding.as_ref(),
    );
    Ok(super::maybe_page_authed("Practices", content, hx, &tenant))
}

async fn query_onboarding_state(
    tenant: &TenantContext,
) -> Result<templates::onboarding::OnboardingState, ErrorResponse> {
    let user_id = tenant
        .claims
        .user_id()
        .ok_or_else(|| internal_error(anyhow::anyhow!("no user id in claims")))?;
    tenant
        .db
        .with_conn(move |conn| {
            let completed = lineup_db::onboarding::completed_steps(conn, user_id)?;
            Ok(templates::onboarding::OnboardingState { completed })
        })
        .await
        .map_err(internal_error)
}

/// Legacy redirect: /practices/planning → /practices
pub(crate) async fn planning_redirect() -> impl IntoResponse {
    Redirect::to("/practices")
}

/// Legacy redirect: /practices/committed → /practices
pub(crate) async fn committed_redirect() -> impl IntoResponse {
    Redirect::to("/practices")
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreatePracticeInput {
    date: String,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    end_time: Option<String>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn create_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Form(input): Form<CreatePracticeInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let date = NaiveDate::parse_from_str(input.date.trim(), "%Y-%m-%d")
        .map_err(|_| bad_request("Invalid date format."))?;
    let time: Option<NaiveTime> = input
        .time
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            NaiveTime::parse_from_str(s.trim(), "%H:%M")
                .map_err(|_| bad_request("Invalid time format."))
        })
        .transpose()?;
    let end_time: Option<NaiveTime> = input
        .end_time
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            NaiveTime::parse_from_str(s.trim(), "%H:%M")
                .map_err(|_| bad_request("Invalid time format."))
        })
        .transpose()?;
    let duration_minutes: Option<lineup_db::types::DurationMinutes> = match (time, end_time) {
        (Some(start), Some(end)) => {
            let dur = end.signed_duration_since(start).num_minutes();
            if dur > 0 {
                Some(lineup_db::types::DurationMinutes::new(dur as i32))
            } else {
                None
            }
        }
        _ => None,
    };

    tenant
        .db
        .with_conn(move |conn| {
            let p = Practice::upsert(conn, team_id, date, time, None)?;
            if let Some(dur) = duration_minutes {
                use diesel::prelude::*;
                diesel::update(lineup_db::schema::practice::table.find(p.id))
                    .set(lineup_db::schema::practice::duration_minutes.eq(Some(dur)))
                    .execute(conn)?;
            }
            Practice::get(conn, p.id)?.ok_or(diesel::result::Error::NotFound)
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "practice.create",
        "practice",
        &date.to_string(),
        None,
    );
    tenant.complete_onboarding_step(lineup_db::onboarding::OnboardingStep::CreatePractice);

    Ok(Redirect::to("/practices"))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn cancel_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let new_cancelled = tenant
        .db
        .with_conn(move |conn| {
            let practice =
                Practice::get(conn, practice_id)?.ok_or(diesel::result::Error::NotFound)?;
            let new_cancelled = !practice.cancelled.as_bool();
            Practice::set_cancelled_by_id(conn, practice_id, new_cancelled)?;
            Ok(new_cancelled)
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "practice.cancel",
        "practice",
        &practice_id.to_string(),
        Some(serde_json::json!({"cancelled": new_cancelled}).to_string()),
    );

    Ok(Redirect::to("/practices"))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn dismiss_plan_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    tenant
        .db
        .with_conn(move |conn| Practice::set_plan_dismissed(conn, practice_id, true))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "practice.dismiss_plan",
        "practice",
        &practice_id.to_string(),
        None,
    );

    let redirect = format!("/history/{practice_id}");
    Ok(Redirect::to(&redirect))
}
