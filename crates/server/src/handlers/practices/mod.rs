//! Practice dashboard handlers: tabs, create, cancel.
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
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect},
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::{NaiveDate, NaiveTime, Utc};
use lineup_db::app_user::{AppUser, Role};
use lineup_db::availability::Availability;
use lineup_db::email_log::EmailLog;
use lineup_db::lineup::Lineup;
use lineup_db::practice::{Practice, PracticeId};
use lineup_db::rower::Rower;
use lineup_db::types::EmailLogType;
use serde::Deserialize;
use std::collections::HashSet;

use crate::state::TenantContext;
use crate::{
    handlers::{bad_request, internal_error, ErrorResponse},
    templates,
};

const TAB_TARGET: &str = "practices-tab-content";

fn is_tab_swap(headers: &HeaderMap) -> bool {
    headers.get("HX-Target").and_then(|v| v.to_str().ok()) == Some(TAB_TARGET)
}

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

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn planning_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    let content = planning_tab_content(&jar, &tenant).await?;
    if is_tab_swap(&headers) {
        return Ok(Html(
            templates::practices::tab_content_swap("planning", content).into_string(),
        ));
    }
    let is_coach = tenant.claims.role().at_least(Role::Coach);
    let page = templates::practices::tabbed_page("planning", content, is_coach, None);
    Ok(super::maybe_page_authed("Practices", page, hx, &tenant))
}

async fn planning_tab_content(
    jar: &CookieJar,
    tenant: &TenantContext,
) -> Result<maud::Markup, ErrorResponse> {
    let team_id = super::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let today = Utc::now().date_naive();
    let is_coach = tenant.claims.role().at_least(Role::Coach);

    let (rows, default_time, default_duration, suggested_date) = tenant
        .db
        .with_conn(move |conn| {
            let team = lineup_db::team::Team::get(conn, team_id)?;
            let default_time = team.as_ref().and_then(|t| t.default_practice_time);
            let default_duration = team
                .as_ref()
                .and_then(|t| t.default_practice_duration_minutes);
            let practice_days = team.as_ref().and_then(|t| t.default_practice_days);
            let upcoming_practices = Practice::list_upcoming(conn, team_id, today)?;

            let upcoming_ids: Vec<PracticeId> = upcoming_practices.iter().map(|p| p.id).collect();
            let committed_ids: HashSet<PracticeId> =
                Practice::committed_ids(conn, team_id, &upcoming_ids)?
                    .into_iter()
                    .collect();
            let draft_ids = Lineup::practices_with_drafts(conn, &upcoming_ids)?;

            let existing_dates: HashSet<chrono::NaiveDate> =
                upcoming_practices.iter().map(|p| p.date).collect();
            let suggested_date =
                practice_days.and_then(|pd| pd.next_unfilled(today, &existing_dates));

            let all_rowers = Rower::list_active(conn)?;
            let rowers_with_user: Vec<_> = all_rowers
                .iter()
                .filter(|r| {
                    AppUser::find_by_rower_id(conn, r.id)
                        .ok()
                        .flatten()
                        .is_some()
                })
                .collect();

            let mut rows = Vec::new();
            for practice in &upcoming_practices {
                if committed_ids.contains(&practice.id) {
                    continue;
                }
                let avail_map = Availability::map_for_practice(conn, practice.id)?;
                let yes = avail_map.values().filter(|s| s.is_available()).count();
                let total = avail_map.len();
                let non_respondents = rowers_with_user
                    .iter()
                    .filter(|r| !avail_map.contains_key(&r.id))
                    .count();
                let already_sent = EmailLog::already_sent_today(
                    conn,
                    team_id,
                    &EmailLogType::new("reminder"),
                    practice.date,
                )?;

                rows.push(templates::practices::PracticeRow {
                    practice_id: practice.id,
                    date: practice.date,
                    time: practice.time,
                    duration_minutes: practice.effective_duration(default_duration),
                    yes_count: yes,
                    total_responses: total,
                    cancelled: practice.cancelled.as_bool(),
                    non_respondent_count: non_respondents,
                    boat_count: 0,
                    already_sent_today: already_sent,
                    has_draft: draft_ids.contains(&practice.id),
                });
            }
            Ok((rows, default_time, default_duration, suggested_date))
        })
        .await
        .map_err(internal_error)?;

    Ok(templates::practices::planning_content(
        &rows,
        is_coach,
        today,
        default_time,
        default_duration,
        suggested_date,
    ))
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
pub(crate) async fn committed_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    let content = committed_tab_content(&jar, &tenant).await?;
    if is_tab_swap(&headers) {
        return Ok(Html(
            templates::practices::tab_content_swap("committed", content).into_string(),
        ));
    }
    let is_coach = tenant.claims.role().at_least(Role::Coach);
    let page = templates::practices::tabbed_page("committed", content, is_coach, None);
    Ok(super::maybe_page_authed("Practices", page, hx, &tenant))
}

async fn committed_tab_content(
    jar: &CookieJar,
    tenant: &TenantContext,
) -> Result<maud::Markup, ErrorResponse> {
    let team_id = super::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let is_coach = tenant.claims.role().at_least(Role::Coach);

    let rows = tenant
        .db
        .with_conn(move |conn| {
            let team = lineup_db::team::Team::get(conn, team_id)?;
            let default_duration = team
                .as_ref()
                .and_then(|t| t.default_practice_duration_minutes);
            let committed_practices = Practice::list_committed(conn, team_id)?;

            let mut rows = Vec::new();
            for practice in &committed_practices {
                let lineups = Lineup::for_practice(conn, practice.id)?;
                if lineups.is_empty() {
                    continue;
                }
                let already_sent = EmailLog::already_sent_today(
                    conn,
                    team_id,
                    &EmailLogType::new("lineup"),
                    practice.date,
                )?;

                rows.push(templates::practices::PracticeRow {
                    practice_id: practice.id,
                    date: practice.date,
                    time: practice.time,
                    duration_minutes: practice.effective_duration(default_duration),
                    yes_count: 0,
                    total_responses: 0,
                    cancelled: practice.cancelled.as_bool(),
                    non_respondent_count: 0,
                    boat_count: lineups.len(),
                    already_sent_today: already_sent,
                    has_draft: false, // committed tab never shows drafts
                });
            }
            rows.sort_by(|a, b| b.date.cmp(&a.date));
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    Ok(templates::practices::committed_content(&rows, is_coach))
}
