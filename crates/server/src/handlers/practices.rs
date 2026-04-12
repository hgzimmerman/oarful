//! `GET /practices` — tabbed dashboard: Schedule, Reminders, Lineups.
//! `POST /practices` — create a practice for a given date.
//! `GET /practices/schedule` — schedule tab content (HTMX partial).
//! `GET /practices/reminders` — reminder tab with non-respondent counts.
//! `POST /practices/send-reminders` — send availability reminders.
//! `GET /practices/lineups` — lineup tab with committed dates.
//! `POST /practices/send-lineups` — send lineup notifications.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::{NaiveDate, Utc};
use lineup_db::app_user::{AppUser, Role};
use lineup_db::availability::Availability;
use lineup_db::boat::Boat;
use lineup_db::email_log::{EmailLog, NewEmailLog};
use lineup_db::lineup::Lineup;
use lineup_db::practice::Practice;
use lineup_db::rower::Rower;
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::extract::HtmlForm;
use crate::mailer::{EmailBoatLineup, EmailLineupSummary, EmailSeat};
use crate::magic_link::create_magic_link;
use crate::state::{AppState, TenantContext};
use crate::{handlers::internal_error, templates};

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

    // Default tab is schedule — render full page with tabs + schedule content.
    let schedule_content = schedule_tab_content(&jar, &tenant).await?;
    let content = templates::practices::tabbed_page(
        "schedule",
        schedule_content,
        is_coach,
    );
    Ok(super::maybe_page_authed("Practices", content, hx, &tenant))
}

// =====================================================================
// Schedule tab
// =====================================================================

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn schedule_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
) -> Result<Html<String>, StatusCode> {
    let content = schedule_tab_content(&jar, &tenant).await?;
    Ok(Html(content.into_string()))
}

async fn schedule_tab_content(
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

    let rows = tenant
        .db
        .with_conn(move |conn| {
            let avail_dates = Availability::upcoming_dates(conn, team_id, today)?;
            let practice_dates = Practice::list_upcoming(conn, team_id, today)?;
            let upcoming: BTreeSet<_> = avail_dates.into_iter().chain(practice_dates).collect();

            let past_committed = Practice::list_committed(conn, team_id)?;
            let past_dates: BTreeSet<_> = past_committed
                .iter()
                .map(|p| p.date)
                .filter(|d| *d < today)
                .collect();

            let all_dates: BTreeSet<_> =
                upcoming.iter().chain(past_dates.iter()).copied().collect();
            let date_vec: Vec<_> = all_dates.iter().copied().collect();
            let committed_dates: HashSet<_> =
                Practice::committed_dates(conn, team_id, &date_vec)?
                    .into_iter()
                    .collect();
            let cancelled_dates: HashSet<NaiveDate> = {
                use diesel::prelude::*;
                use lineup_db::schema::practice as p;
                p::table
                    .filter(p::team_id.eq(team_id))
                    .filter(p::date.eq_any(&date_vec))
                    .filter(p::cancelled.ne(0))
                    .select(p::date)
                    .get_results::<NaiveDate>(conn)?
                    .into_iter()
                    .collect()
            };

            let mut rows = Vec::with_capacity(all_dates.len());
            for date in all_dates.iter().rev() {
                let (yes_count, total_responses) = if *date >= today {
                    let map = Availability::map_for_team_date(conn, team_id, *date)?;
                    let yes = map.values().filter(|s| s.is_available_for_sweep()).count();
                    (yes, map.len())
                } else {
                    (0, 0)
                };
                rows.push(templates::practices::PracticeRow {
                    date: *date,
                    yes_count,
                    total_responses,
                    has_committed: committed_dates.contains(date),
                    is_upcoming: *date >= today,
                    cancelled: cancelled_dates.contains(date),
                });
            }
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    Ok(templates::practices::schedule_content(&rows, is_coach))
}

// =====================================================================
// Create + Cancel (unchanged)
// =====================================================================

#[derive(Debug, Deserialize)]
pub(crate) struct CreatePracticeInput {
    date: String,
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

    tenant
        .db
        .with_conn(move |conn| Practice::upsert_by_date(conn, team_id, date, None))
        .await
        .map_err(internal_error)?;

    Ok(Redirect::to("/practices"))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn cancel_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(date): Path<NaiveDate>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    tenant
        .db
        .with_conn(move |conn| {
            let practice = Practice::find_by_date(conn, team_id, date)?
                .ok_or(diesel::result::Error::NotFound)?;
            let new_cancelled = !practice.cancelled.as_bool();
            Practice::set_cancelled(conn, team_id, date, new_cancelled)
        })
        .await
        .map_err(internal_error)?;

    Ok(Redirect::to("/practices"))
}

// =====================================================================
// Reminders tab
// =====================================================================

/// Data for one row in the reminders tab.
pub(crate) struct ReminderRow {
    pub(crate) date: NaiveDate,
    pub(crate) non_respondent_count: usize,
    pub(crate) already_sent_today: bool,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn reminders_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let today = Utc::now().date_naive();

    let rows = tenant
        .db
        .with_conn(move |conn| {
            let upcoming = Practice::list_upcoming(conn, team_id, today)?;
            // Filter to non-cancelled, uncommitted dates.
            let committed: HashSet<_> =
                Practice::committed_dates(conn, team_id, &upcoming)?
                    .into_iter()
                    .collect();

            let all_rowers = Rower::list_active(conn)?;
            let rowers_with_user: Vec<_> = all_rowers
                .iter()
                .filter(|r| r.user_id.is_some())
                .collect();

            let mut rows = Vec::new();
            for date in &upcoming {
                if committed.contains(date) {
                    continue;
                }
                let responses = Availability::map_for_team_date(conn, team_id, *date)?;
                let non_respondents = rowers_with_user
                    .iter()
                    .filter(|r| !responses.contains_key(&r.id))
                    .count();

                let already_sent =
                    EmailLog::already_sent_today(conn, team_id, "reminder", *date)?;

                rows.push(ReminderRow {
                    date: *date,
                    non_respondent_count: non_respondents,
                    already_sent_today: already_sent,
                });
            }
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    Ok(Html(
        templates::practices::reminders_content(&rows).into_string(),
    ))
}

// =====================================================================
// Send reminders
// =====================================================================

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn send_reminders_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let today = Utc::now().date_naive();
    let sender_id = tenant.claims.user_id();
    let team_name = tenant.config.tenant_name.clone();

    // Gather non-respondent users + their pending dates.
    let recipients = tenant
        .db
        .with_conn(move |conn| {
            let upcoming = Practice::list_upcoming(conn, team_id, today)?;
            let committed: HashSet<_> =
                Practice::committed_dates(conn, team_id, &upcoming)?
                    .into_iter()
                    .collect();
            let pending_dates: Vec<NaiveDate> = upcoming
                .into_iter()
                .filter(|d| !committed.contains(d))
                .collect();

            if pending_dates.is_empty() {
                return Ok(Vec::new());
            }

            let all_rowers = Rower::list_active(conn)?;
            // Build rower_id → (user_id, email, name) for rowers with user accounts.
            let mut rower_users: HashMap<
                lineup_db::rower::types::RowerId,
                (lineup_db::app_user::UserId, String, String),
            > = HashMap::new();
            for r in &all_rowers {
                if let (Some(uid), Some(email)) = (r.user_id, r.email.as_ref()) {
                    let user_id = lineup_db::app_user::UserId::new(uid);
                    // Check opt-in.
                    if let Some(user) = AppUser::get(conn, user_id)? {
                        if user.wants_reminders() && user.parsed_status() == Some(lineup_db::app_user::UserStatus::Active) {
                            rower_users.insert(r.id, (user_id, email.clone(), r.name.clone()));
                        }
                    }
                }
            }

            // For each rower, find which pending dates they haven't responded to.
            let mut result: Vec<(lineup_db::app_user::UserId, String, String, Vec<NaiveDate>)> =
                Vec::new();
            for (rower_id, (user_id, email, name)) in &rower_users {
                let mut missing = Vec::new();
                for date in &pending_dates {
                    let responses = Availability::map_for_team_date(conn, team_id, *date)?;
                    if !responses.contains_key(rower_id) {
                        // Check rate limit per date.
                        if !EmailLog::already_sent_today(conn, team_id, "reminder", *date)? {
                            missing.push(*date);
                        }
                    }
                }
                if !missing.is_empty() {
                    result.push((*user_id, email.clone(), name.clone(), missing));
                }
            }

            // Record sends.
            let now = Utc::now().naive_utc();
            let mut dates_sent: HashSet<NaiveDate> = HashSet::new();
            for (_uid, _email, _name, dates) in &result {
                for date in dates {
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
                                    .filter(|(_, _, _, ds)| ds.contains(date))
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
    for (user_id, email, name, dates) in &recipients {
        // Magic link expires end-of-day of the last relevant date.
        let last_date = dates.iter().max().copied().unwrap();
        let expires_at = last_date.and_hms_opt(23, 59, 59).unwrap();

        let created = create_magic_link(*user_id, "/my/availability", expires_at);
        let raw_token = created.raw_token.clone();

        // Insert the magic link row.
        let row = created.row;
        tenant
            .db
            .with_conn(move |conn| lineup_db::magic_link::MagicLink::create(conn, row))
            .await
            .map_err(internal_error)?;

        let magic_url = state.full_url(&format!("/auth/magic/{raw_token}"));
        if let Err(err) = state
            .mailer
            .send_reminder(&email, &name, &team_name, dates, &magic_url)
            .await
        {
            tracing::warn!(?err, %email, "failed to send reminder");
        } else {
            sent_count += 1;
        }
    }

    let msg = if sent_count > 0 {
        format!("Reminders sent to {sent_count} rower(s).")
    } else {
        "No reminders to send -- everyone has responded or reminders were already sent today."
            .to_string()
    };
    Ok(Html(
        templates::practices::send_result(&msg).into_string(),
    ))
}

// =====================================================================
// Lineups tab
// =====================================================================

/// Data for one row in the lineups tab.
pub(crate) struct LineupRow {
    pub(crate) date: NaiveDate,
    pub(crate) boat_count: usize,
    pub(crate) already_sent_today: bool,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn lineups_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let today = Utc::now().date_naive();

    let rows = tenant
        .db
        .with_conn(move |conn| {
            let upcoming = Practice::list_upcoming(conn, team_id, today)?;
            let committed: HashSet<_> =
                Practice::committed_dates(conn, team_id, &upcoming)?
                    .into_iter()
                    .collect();

            let mut rows = Vec::new();
            for date in &upcoming {
                if !committed.contains(date) {
                    continue;
                }
                let practice = Practice::find_by_date(conn, team_id, *date)?
                    .ok_or(diesel::result::Error::NotFound)?;
                let lineups = Lineup::for_practice(conn, practice.id)?;
                let already_sent =
                    EmailLog::already_sent_today(conn, team_id, "lineup", *date)?;
                rows.push(LineupRow {
                    date: *date,
                    boat_count: lineups.len(),
                    already_sent_today: already_sent,
                });
            }
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    Ok(Html(
        templates::practices::lineups_content(&rows).into_string(),
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
            templates::practices::send_result("No valid dates selected.").into_string(),
        ));
    }

    // Build lineup summaries + recipient lists.
    let (summaries, recipients) = tenant
        .db
        .with_conn(move |conn| {
            let all_rowers = Rower::list_active(conn)?;
            let rower_map: HashMap<lineup_db::rower::types::RowerId, &Rower> =
                all_rowers.iter().map(|r| (r.id, r)).collect();

            let mut summaries = Vec::new();
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
                let available = Availability::map_for_team_date(conn, team_id, *date)?;
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
                return Ok((summaries, Vec::new()));
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
                    let responses = Availability::map_for_team_date(conn, team_id, *date)?;
                    for r in &all_rowers {
                        if r.user_id.is_some() && !recipient_rower_ids.contains(&r.id) {
                            // Include if no response at all.
                            if !responses.contains_key(&r.id) {
                                recipient_rower_ids.insert(r.id);
                            }
                        }
                    }
                }
            }

            // Resolve to (user_id, email, name) with opt-in check.
            let mut recipients: Vec<(lineup_db::app_user::UserId, String, String)> = Vec::new();
            for rid in &recipient_rower_ids {
                if let Some(r) = rower_map.get(rid) {
                    if let (Some(uid), Some(email)) = (r.user_id, r.email.as_ref()) {
                        let user_id = lineup_db::app_user::UserId::new(uid);
                        if let Some(user) = AppUser::get(conn, user_id)? {
                            if user.wants_lineups()
                                && user.parsed_status()
                                    == Some(lineup_db::app_user::UserStatus::Active)
                            {
                                recipients.push((user_id, email.clone(), r.name.clone()));
                            }
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

            Ok((summaries, recipients))
        })
        .await
        .map_err(internal_error)?;

    // Send emails.
    let mut sent_count = 0;
    if !summaries.is_empty() {
        // Magic link expires end-of-day of the last date.
        let last_date = summaries.iter().map(|s| s.date).max().unwrap();
        let expires_at = last_date.and_hms_opt(23, 59, 59).unwrap();

        for (user_id, email, name) in &recipients {
            // Each user gets their own magic link (different tokens).
            let first_date = summaries.first().map(|s| s.date).unwrap();
            let redirect = format!("/history/{first_date}");
            let created = create_magic_link(*user_id, &redirect, expires_at);
            let raw_token = created.raw_token.clone();
            let row = created.row;

            tenant
                .db
                .with_conn(move |conn| lineup_db::magic_link::MagicLink::create(conn, row))
                .await
                .map_err(internal_error)?;

            let magic_url = state.full_url(&format!("/auth/magic/{raw_token}"));
            if let Err(err) = state
                .mailer
                .send_lineup(&email, &name, &team_name, &summaries, &magic_url)
                .await
            {
                tracing::warn!(?err, %email, "failed to send lineup");
            } else {
                sent_count += 1;
            }
        }
    }

    let msg = if sent_count > 0 {
        format!("Lineup notifications sent to {sent_count} rower(s).")
    } else {
        "No lineup notifications to send -- no committed dates selected or already sent today."
            .to_string()
    };
    Ok(Html(
        templates::practices::send_result(&msg).into_string(),
    ))
}
