//! `GET /practices` — tabbed dashboard: Planning, Committed.
//! `POST /practices` — create a practice for a given date.
//! `GET /practices/planning` — planning tab content (HTMX partial).
//! `GET /practices/committed` — committed tab content (HTMX partial).
//! `GET /practices/reminder-preview` — preview modal for reminders.
//! `POST /practices/send-reminders` — send availability reminders.
//! `POST /practices/send-lineups` — send lineup notifications.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::{NaiveDate, NaiveTime, Utc};
use lineup_db::app_user::{AppUser, Role};
use lineup_db::availability::Availability;
use lineup_db::boat::Boat;
use lineup_db::email_log::{EmailLog, NewEmailLog};
use lineup_db::lineup::Lineup;
use lineup_db::practice::{Practice, PracticeId};
use lineup_db::rower::Rower;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

use crate::extract::HtmlForm;
use crate::mailer::{EmailBoatLineup, EmailLineupSummary, EmailSeat};
use crate::magic_link::create_magic_link;
use crate::state::{AppState, TenantContext};
use crate::{handlers::internal_error, templates};

const TAB_TARGET: &str = "practices-tab-content";

fn is_tab_swap(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Target")
        .and_then(|v| v.to_str().ok())
        == Some(TAB_TARGET)
}

// =====================================================================
// Main practices page (with tabs)
// =====================================================================

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn list_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let is_coach = tenant
        .claims
        .role()
        .unwrap_or(Role::Member)
        .at_least(Role::Coach);

    // Default tab is planning — render full page with tabs + planning content.
    let planning_content = planning_tab_content(&jar, &tenant).await?;
    let content = templates::practices::tabbed_page(
        "planning",
        planning_content,
        is_coach,
    );
    Ok(super::maybe_page_authed("Practices", content, hx, &tenant))
}

// =====================================================================
// Planning tab
// =====================================================================

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn planning_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    let content = planning_tab_content(&jar, &tenant).await?;
    if is_tab_swap(&headers) {
        return Ok(Html(
            templates::practices::tab_content_swap("planning", content).into_string(),
        ));
    }
    let is_coach = tenant.claims.role().unwrap_or(Role::Member).at_least(Role::Coach);
    let page = templates::practices::tabbed_page("planning", content, is_coach);
    Ok(super::maybe_page_authed("Practices", page, hx, &tenant))
}

async fn planning_tab_content(
    jar: &CookieJar,
    tenant: &TenantContext,
) -> Result<maud::Markup, StatusCode> {
    let team_id = super::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let today = Utc::now().date_naive();
    let is_coach = tenant
        .claims
        .role()
        .unwrap_or(Role::Member)
        .at_least(Role::Coach);

    let (rows, default_time, default_duration, suggested_date) = tenant
        .db
        .with_conn(move |conn| {
            let team = lineup_db::team::Team::get(conn, team_id)?;
            let default_time = team.as_ref().and_then(|t| t.default_practice_time);
            let default_duration = team.as_ref().and_then(|t| t.default_practice_duration_minutes);
            let practice_days = team.as_ref().and_then(|t| t.default_practice_days);
            // Upcoming practices (non-cancelled) — already ascending by date.
            let upcoming_practices = Practice::list_upcoming(conn, team_id, today)?;

            let upcoming_ids: Vec<PracticeId> = upcoming_practices.iter().map(|p| p.id).collect();
            let committed_ids: HashSet<PracticeId> =
                Practice::committed_ids(conn, team_id, &upcoming_ids)?
                    .into_iter()
                    .collect();

            // Compute next suggested date from practice-day defaults.
            let existing_dates: HashSet<chrono::NaiveDate> =
                upcoming_practices.iter().map(|p| p.date).collect();
            let suggested_date = practice_days
                .and_then(|pd| pd.next_unfilled(today, &existing_dates));

            // Rowers with user accounts (for non-respondent count).
            let all_rowers = Rower::list_active(conn)?;
            let rowers_with_user: Vec<_> = all_rowers
                .iter()
                .filter(|r| AppUser::find_by_rower_id(conn, r.id).ok().flatten().is_some())
                .collect();

            let mut rows = Vec::new();
            for practice in &upcoming_practices {
                // Planning tab: only practices WITHOUT committed lineups.
                if committed_ids.contains(&practice.id) {
                    continue;
                }
                let avail_map = Availability::map_for_practice(conn, practice.id)?;
                let yes = avail_map.values().filter(|s| s.is_available_for_sweep()).count();
                let total = avail_map.len();
                let non_respondents = rowers_with_user
                    .iter()
                    .filter(|r| !avail_map.contains_key(&r.id))
                    .count();
                let already_sent =
                    EmailLog::already_sent_today(conn, team_id, "reminder", practice.date)?;

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
                });
            }
            Ok((rows, default_time, default_duration, suggested_date))
        })
        .await
        .map_err(internal_error)?;

    Ok(templates::practices::planning_content(&rows, is_coach, today, default_time, default_duration, suggested_date))
}

// =====================================================================
// Create + Cancel (unchanged)
// =====================================================================

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
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let date = NaiveDate::parse_from_str(input.date.trim(), "%Y-%m-%d")
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let time: Option<NaiveTime> = input.time
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| NaiveTime::parse_from_str(s.trim(), "%H:%M").map_err(|_| StatusCode::BAD_REQUEST))
        .transpose()?;
    let end_time: Option<NaiveTime> = input.end_time
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| NaiveTime::parse_from_str(s.trim(), "%H:%M").map_err(|_| StatusCode::BAD_REQUEST))
        .transpose()?;
    // Compute duration from start + end times.
    let duration_minutes: Option<i32> = match (time, end_time) {
        (Some(start), Some(end)) => {
            let dur = end.signed_duration_since(start).num_minutes();
            if dur > 0 { Some(dur as i32) } else { None }
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
        Some(tenant.claims.user_id().as_int()),
        "practice.create",
        "practice",
        &date.to_string(),
        None,
    );

    Ok(Redirect::to("/practices"))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn cancel_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let new_cancelled = tenant
        .db
        .with_conn(move |conn| {
            let practice = Practice::get(conn, practice_id)?
                .ok_or(diesel::result::Error::NotFound)?;
            let new_cancelled = !practice.cancelled.as_bool();
            Practice::set_cancelled_by_id(conn, practice_id, new_cancelled)?;
            Ok(new_cancelled)
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "practice.cancel",
        "practice",
        &practice_id.to_string(),
        Some(serde_json::json!({"cancelled": new_cancelled}).to_string()),
    );

    Ok(Redirect::to("/practices"))
}

// =====================================================================
// Committed tab
// =====================================================================

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn committed_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, StatusCode> {
    let content = committed_tab_content(&jar, &tenant).await?;
    if is_tab_swap(&headers) {
        return Ok(Html(
            templates::practices::tab_content_swap("committed", content).into_string(),
        ));
    }
    let is_coach = tenant.claims.role().unwrap_or(Role::Member).at_least(Role::Coach);
    let page = templates::practices::tabbed_page("committed", content, is_coach);
    Ok(super::maybe_page_authed("Practices", page, hx, &tenant))
}

async fn committed_tab_content(
    jar: &CookieJar,
    tenant: &TenantContext,
) -> Result<maud::Markup, StatusCode> {
    let team_id = super::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let is_coach = tenant
        .claims
        .role()
        .unwrap_or(Role::Member)
        .at_least(Role::Coach);

    let rows = tenant
        .db
        .with_conn(move |conn| {
            let team = lineup_db::team::Team::get(conn, team_id)?;
            let default_duration = team.as_ref().and_then(|t| t.default_practice_duration_minutes);
            // All committed practices (past + upcoming).
            let committed_practices = Practice::list_committed(conn, team_id)?;

            let mut rows = Vec::new();
            for practice in &committed_practices {
                let lineups = Lineup::for_practice(conn, practice.id)?;
                if lineups.is_empty() {
                    continue;
                }
                let already_sent =
                    EmailLog::already_sent_today(conn, team_id, "lineup", practice.date)?;

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
                });
            }
            // Sort descending by date (newest first).
            rows.sort_by(|a, b| b.date.cmp(&a.date));
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    Ok(templates::practices::committed_content(&rows, is_coach))
}

// =====================================================================
// Reminder helpers
// =====================================================================

/// Per-practice non-respondent info shared by preview and send.
struct ReminderRecipient {
    user_id: lineup_db::app_user::UserId,
    email: String,
    name: String,
    dates: Vec<NaiveDate>,
}

/// Gather non-respondent users for the given practices (or all pending
/// when `practice_ids` is empty). Returns one entry per user with the
/// list of dates they haven't responded to.
fn gather_reminder_recipients(
    conn: &mut diesel::SqliteConnection,
    team_id: lineup_db::team::TeamId,
    practice_ids: &[PracticeId],
    today: NaiveDate,
) -> Result<Vec<ReminderRecipient>, diesel::result::Error> {
    let upcoming = Practice::list_upcoming(conn, team_id, today)?;
    let upcoming_ids: Vec<PracticeId> = upcoming.iter().map(|p| p.id).collect();
    let committed: HashSet<PracticeId> = Practice::committed_ids(conn, team_id, &upcoming_ids)?
        .into_iter()
        .collect();
    let pending: Vec<&Practice> = upcoming
        .iter()
        .filter(|p| !committed.contains(&p.id))
        .filter(|p| practice_ids.is_empty() || practice_ids.contains(&p.id))
        .collect();

    if pending.is_empty() {
        return Ok(Vec::new());
    }

    let all_rowers = Rower::list_active(conn)?;
    let mut rower_users: HashMap<
        lineup_db::rower::types::RowerId,
        (lineup_db::app_user::UserId, String, String),
    > = HashMap::new();
    for r in &all_rowers {
        if let Some(user) = AppUser::find_by_rower_id(conn, r.id)? {
            if user.wants_reminders()
                && user.parsed_status() == Some(lineup_db::app_user::UserStatus::Active)
            {
                rower_users.insert(r.id, (user.id, user.email.clone(), r.name.clone()));
            }
        }
    }

    let mut result: Vec<ReminderRecipient> = Vec::new();
    for (rower_id, (user_id, email, name)) in &rower_users {
        let mut missing = Vec::new();
        for practice in &pending {
            let responses = Availability::map_for_practice(conn, practice.id)?;
            if !responses.contains_key(rower_id) {
                if !EmailLog::already_sent_today(conn, team_id, "reminder", practice.date)? {
                    missing.push(practice.date);
                }
            }
        }
        if !missing.is_empty() {
            result.push(ReminderRecipient {
                user_id: *user_id,
                email: email.clone(),
                name: name.clone(),
                dates: missing,
            });
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

// =====================================================================
// Reminder preview
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct ReminderPreviewQuery {
    #[serde(default)]
    practice_ids: Vec<i32>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn reminder_preview_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    crate::extract::HtmlQuery(query): crate::extract::HtmlQuery<ReminderPreviewQuery>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let today = Utc::now().date_naive();
    let pids: Vec<PracticeId> = query.practice_ids.iter().map(|&id| PracticeId::new(id)).collect();

    let recipients = tenant
        .db
        .with_conn(move |conn| gather_reminder_recipients(conn, team_id, &pids, today))
        .await
        .map_err(internal_error)?;

    let preview: Vec<templates::practices::ReminderRecipientPreview> = recipients
        .iter()
        .map(|r| templates::practices::ReminderRecipientPreview {
            name: r.name.clone(),
            dates: r.dates.clone(),
        })
        .collect();

    Ok(Html(
        templates::practices::reminder_preview_modal(&preview, &query.practice_ids).into_string(),
    ))
}

// =====================================================================
// Send reminders
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct SendRemindersInput {
    #[serde(default)]
    practice_ids: Vec<i32>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn send_reminders_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    HtmlForm(input): HtmlForm<SendRemindersInput>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let today = Utc::now().date_naive();
    let sender_id = tenant.claims.user_id();
    let team_name = tenant.config.tenant_name.clone();
    let pids: Vec<PracticeId> = input.practice_ids.iter().map(|&id| PracticeId::new(id)).collect();

    // Gather non-respondent users + record sends.
    let recipients = tenant
        .db
        .with_conn(move |conn| {
            let result = gather_reminder_recipients(conn, team_id, &pids, today)?;

            // Record sends.
            let now = Utc::now().naive_utc();
            let mut dates_sent: HashSet<NaiveDate> = HashSet::new();
            for r in &result {
                for date in &r.dates {
                    if dates_sent.insert(*date) {
                        EmailLog::record(
                            conn,
                            NewEmailLog {
                                team_id,
                                email_type: "reminder".to_string(),
                                practice_date: *date,
                                sent_at: now,
                                recipient_count: result
                                    .iter()
                                    .filter(|r2| r2.dates.contains(date))
                                    .count() as i32,
                                sent_by_user_id: sender_id,
                            },
                        )?;
                    }
                }
            }

            Ok(result)
        })
        .await
        .map_err(internal_error)?;

    // Send emails (outside the DB transaction).
    let mut sent_count = 0;
    let mut sent_names: Vec<String> = Vec::new();
    for r in &recipients {
        // Magic link expires end-of-day of the last relevant date.
        let last_date = r.dates.iter().max().copied().unwrap();
        let expires_at = last_date.and_hms_opt(23, 59, 59).unwrap();

        let created = create_magic_link(r.user_id, "/my/availability", expires_at, Some(team_id));
        let raw_token = created.raw_token.clone();

        // Insert the magic link row.
        let row = created.row;
        tenant
            .db
            .with_conn(move |conn| lineup_db::magic_link::MagicLink::create(conn, row))
            .await
            .map_err(internal_error)?;

        let magic_url = state.full_url(&format!("/auth/magic/{}/{raw_token}", tenant.config.tenant_slug));
        if let Err(err) = state
            .mailer
            .send_reminder(&r.email, &r.name, &team_name, &r.dates, &magic_url)
            .await
        {
            tracing::warn!(?err, email = %r.email, "failed to send reminder");
        } else {
            sent_count += 1;
            sent_names.push(r.name.clone());
        }
    }

    if sent_count > 0 {
        crate::audit::record(
            &tenant.db,
            Some(tenant.claims.user_id().as_int()),
            "practices.send_reminders",
            "practice",
            &format!("{sent_count} recipients"),
            Some(serde_json::json!({"sent_count": sent_count}).to_string()),
        );
    }

    let msg = if sent_count > 0 {
        format!("Reminders sent to: {}", sent_names.join(", "))
    } else {
        "No reminders to send — everyone has responded or reminders were already sent today."
            .to_string()
    };
    Ok(Html(
        templates::practices::send_result(&msg).into_string(),
    ))
}

// =====================================================================
// Send lineups
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct SendLineupsInput {
    /// Dates selected via checkboxes (repeated `dates` fields).
    #[serde(default)]
    pub(crate) dates: Vec<String>,
    /// "placed" or "all" -- recipient scope.
    #[serde(default = "default_scope")]
    pub(crate) scope: String,
}

fn default_scope() -> String {
    "placed".to_string()
}

// =====================================================================
// Lineup preview
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct LineupPreviewQuery {
    #[serde(default)]
    dates: Vec<String>,
    #[serde(default = "default_scope")]
    scope: String,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn lineup_preview_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    crate::extract::HtmlQuery(query): crate::extract::HtmlQuery<LineupPreviewQuery>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let scope_all = query.scope == "all";

    let dates: Vec<NaiveDate> = query
        .dates
        .iter()
        .filter_map(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
        .collect();

    let date_strs: Vec<String> = dates.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect();

    if dates.is_empty() {
        return Ok(Html(
            templates::practices::lineup_preview_modal(&[], &date_strs, "placed").into_string(),
        ));
    }

    let scope = query.scope.clone();
    let recipients = tenant
        .db
        .with_conn(move |conn| {
            gather_lineup_recipients(conn, team_id, &dates, scope_all)
        })
        .await
        .map_err(internal_error)?;

    let preview: Vec<templates::practices::LineupRecipientPreview> = recipients
        .iter()
        .map(|(_uid, _email, name)| templates::practices::LineupRecipientPreview {
            name: name.clone(),
        })
        .collect();

    Ok(Html(
        templates::practices::lineup_preview_modal(&preview, &date_strs, &scope).into_string(),
    ))
}

/// Gather lineup email recipients for the given dates and scope.
/// Returns (user_id, email, name) tuples.
fn gather_lineup_recipients(
    conn: &mut diesel::SqliteConnection,
    team_id: lineup_db::team::TeamId,
    dates: &[NaiveDate],
    scope_all: bool,
) -> Result<Vec<(lineup_db::app_user::UserId, String, String)>, diesel::result::Error> {
    let all_rowers = Rower::list_active(conn)?;
    let rower_map: HashMap<lineup_db::rower::types::RowerId, &Rower> =
        all_rowers.iter().map(|r| (r.id, r)).collect();

    let mut placed_rower_ids: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();
    let mut benched_rower_ids: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();
    let mut valid_dates = Vec::new();

    for date in dates {
        if EmailLog::already_sent_today(conn, team_id, "lineup", *date)? {
            continue;
        }
        let practice = match Practice::find_by_date(conn, team_id, *date)? {
            Some(p) => p,
            None => continue,
        };
        let committed = lineup_db::lineup::Lineup::for_practice(conn, practice.id)?;
        if committed.is_empty() {
            continue;
        }
        valid_dates.push(*date);

        for cl in &committed {
            for seat_row in &cl.seats {
                placed_rower_ids.insert(seat_row.rower_id);
            }
        }

        let available = Availability::map_for_practice(conn, practice.id)?;
        for (rid, status) in &available {
            if status.is_available_for_sweep() && !placed_rower_ids.contains(rid) {
                benched_rower_ids.insert(*rid);
            }
        }
    }

    let mut recipient_rower_ids: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();
    recipient_rower_ids.extend(&placed_rower_ids);
    recipient_rower_ids.extend(&benched_rower_ids);

    if scope_all {
        for date in &valid_dates {
            if let Some(practice) = Practice::find_by_date(conn, team_id, *date)? {
                let responses = Availability::map_for_practice(conn, practice.id)?;
                for r in &all_rowers {
                    if !recipient_rower_ids.contains(&r.id)
                        && AppUser::find_by_rower_id(conn, r.id)?.is_some()
                        && !responses.contains_key(&r.id)
                    {
                        recipient_rower_ids.insert(r.id);
                    }
                }
            }
        }
    }

    let mut recipients = Vec::new();
    for rid in &recipient_rower_ids {
        if let Some(r) = rower_map.get(rid) {
            if let Some(user) = AppUser::find_by_rower_id(conn, r.id)? {
                if user.wants_lineups()
                    && user.parsed_status() == Some(lineup_db::app_user::UserStatus::Active)
                {
                    recipients.push((user.id, user.email.clone(), r.name.clone()));
                }
            }
        }
    }
    recipients.sort_by(|a, b| a.2.cmp(&b.2));
    Ok(recipients)
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn send_lineups_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    HtmlForm(input): HtmlForm<SendLineupsInput>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let sender_id = tenant.claims.user_id();
    let team_name = tenant.config.tenant_name.clone();
    let scope_all = input.scope == "all";

    // Parse requested dates from checkbox values.
    let dates: Vec<NaiveDate> = input
        .dates
        .iter()
        .filter_map(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
        .collect();
    if dates.is_empty() {
        return Ok(Html(
            templates::practices::send_warning("No practices selected — check at least one to send lineups.").into_string(),
        ));
    }

    // Build lineup summaries + recipient lists.
    let (summaries, recipients, first_practice_id) = tenant
        .db
        .with_conn(move |conn| {
            let all_rowers = Rower::list_active(conn)?;
            let rower_map: HashMap<lineup_db::rower::types::RowerId, &Rower> =
                all_rowers.iter().map(|r| (r.id, r)).collect();

            let mut summaries = Vec::new();
            let mut first_practice_id: Option<PracticeId> = None;
            // Track all placed + benched rower IDs across all dates.
            let mut placed_rower_ids: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();
            let mut benched_rower_ids: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();

            for date in &dates {
                // Rate limit check.
                if EmailLog::already_sent_today(conn, team_id, "lineup", *date)? {
                    continue;
                }

                let practice = match Practice::find_by_date(conn, team_id, *date)? {
                    Some(p) => p,
                    None => continue,
                };
                let committed = Lineup::for_practice(conn, practice.id)?;
                if committed.is_empty() {
                    continue;
                }
                if first_practice_id.is_none() {
                    first_practice_id = Some(practice.id);
                }

                // Build email-friendly lineup data.
                let mut boats = Vec::new();
                let mut date_placed: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();
                for cl in &committed {
                    let boat = Boat::get(conn, cl.lineup.boat_id)?;
                    let boat_name = boat.map(|b| b.name.clone()).unwrap_or_else(|| "Unknown".to_string());

                    let mut seats = Vec::new();
                    for seat_row in &cl.seats {
                        let rower_name = rower_map
                            .get(&seat_row.rower_id)
                            .map(|r| r.name.clone())
                            .unwrap_or_else(|| "?".to_string());
                        let label = if seat_row.is_cox.as_bool() {
                            "Cox".to_string()
                        } else {
                            format!("Seat {}", seat_row.seat_position)
                        };
                        seats.push(EmailSeat {
                            label,
                            rower_name,
                        });
                        date_placed.insert(seat_row.rower_id);
                    }

                    boats.push(EmailBoatLineup { boat_name, seats });
                }

                // Benched = available rowers not placed in any boat.
                let available = Availability::map_for_practice(conn, practice.id)?;
                let mut benched_names = Vec::new();
                let mut date_benched: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();
                for (rid, status) in &available {
                    if status.is_available_for_sweep() && !date_placed.contains(rid) {
                        if let Some(r) = rower_map.get(rid) {
                            benched_names.push(r.name.clone());
                            date_benched.insert(*rid);
                        }
                    }
                }

                placed_rower_ids.extend(date_placed);
                benched_rower_ids.extend(date_benched);

                summaries.push(EmailLineupSummary {
                    date: *date,
                    boats,
                    benched: benched_names,
                });
            }

            if summaries.is_empty() {
                return Ok((summaries, Vec::new(), first_practice_id));
            }

            // Determine recipients based on scope.
            // "placed" = rowers in boats + benched rowers
            // "all" = placed + benched + non-respondents (except explicit No)
            let mut recipient_rower_ids: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();
            recipient_rower_ids.extend(&placed_rower_ids);
            recipient_rower_ids.extend(&benched_rower_ids);

            if scope_all {
                // Add non-respondents (those who haven't said No).
                for date in &dates {
                    if let Some(practice) = Practice::find_by_date(conn, team_id, *date)? {
                        let responses = Availability::map_for_practice(conn, practice.id)?;
                        for r in &all_rowers {
                            if !recipient_rower_ids.contains(&r.id) {
                                // Include if has a user account and no response at all.
                                if AppUser::find_by_rower_id(conn, r.id)?.is_some()
                                    && !responses.contains_key(&r.id)
                                {
                                    recipient_rower_ids.insert(r.id);
                                }
                            }
                        }
                    }
                }
            }

            // Resolve to (user_id, email, name) with opt-in check.
            let mut recipients: Vec<(lineup_db::app_user::UserId, String, String)> = Vec::new();
            for rid in &recipient_rower_ids {
                if let Some(r) = rower_map.get(rid) {
                    if let Some(user) = AppUser::find_by_rower_id(conn, r.id)? {
                        if user.wants_lineups()
                            && user.parsed_status()
                                == Some(lineup_db::app_user::UserStatus::Active)
                        {
                            recipients.push((user.id, user.email.clone(), r.name.clone()));
                        }
                    }
                }
            }

            // Record sends.
            let now = Utc::now().naive_utc();
            for summary in &summaries {
                EmailLog::record(
                    conn,
                    NewEmailLog {
                        team_id,
                        email_type: "lineup".to_string(),
                        practice_date: summary.date,
                        sent_at: now,
                        recipient_count: recipients.len() as i32,
                        sent_by_user_id: sender_id,
                    },
                )?;
            }

            Ok((summaries, recipients, first_practice_id))
        })
        .await
        .map_err(internal_error)?;

    // Send emails.
    let mut sent_count = 0;
    let mut sent_names: Vec<String> = Vec::new();
    if !summaries.is_empty() {
        // Magic link expires end-of-day of the last date.
        let last_date = summaries.iter().map(|s| s.date).max().unwrap();
        let expires_at = last_date.and_hms_opt(23, 59, 59).unwrap();

        for (user_id, email, name) in &recipients {
            // Each user gets their own magic link (different tokens).
            let redirect = match first_practice_id {
                Some(pid) => format!("/history/{pid}"),
                None => "/history".to_string(),
            };
            let created = create_magic_link(*user_id, &redirect, expires_at, Some(team_id));
            let raw_token = created.raw_token.clone();
            let row = created.row;

            tenant
                .db
                .with_conn(move |conn| lineup_db::magic_link::MagicLink::create(conn, row))
                .await
                .map_err(internal_error)?;

            let magic_url = state.full_url(&format!("/auth/magic/{}/{raw_token}", tenant.config.tenant_slug));
            if let Err(err) = state
                .mailer
                .send_lineup(&email, &name, &team_name, &summaries, &magic_url)
                .await
            {
                tracing::warn!(?err, %email, "failed to send lineup");
            } else {
                sent_count += 1;
                sent_names.push(name.clone());
            }
        }
    }

    if sent_count > 0 {
        let date_strs: Vec<String> = summaries.iter().map(|s| s.date.to_string()).collect();
        crate::audit::record(
            &tenant.db,
            Some(tenant.claims.user_id().as_int()),
            "practices.send_lineups",
            "practice",
            &date_strs.join(","),
            Some(serde_json::json!({"sent_count": sent_count, "dates": date_strs}).to_string()),
        );
    }

    let msg = if sent_count > 0 {
        format!("Lineups sent to: {}", sent_names.join(", "))
    } else {
        "No lineup notifications to send — lineups may have already been sent today."
            .to_string()
    };
    Ok(Html(
        templates::practices::send_result(&msg).into_string(),
    ))
}
