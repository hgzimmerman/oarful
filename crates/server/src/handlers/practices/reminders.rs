//! Reminder preview + send handlers.

use axum::{extract::State, response::Html, Extension};
use axum_extra::extract::CookieJar;
use chrono::{NaiveDate, Utc};
use lineup_db::app_user::{AppUser, Role};
use lineup_db::availability::Availability;
use lineup_db::email_log::{EmailLog, NewEmailLog};
use lineup_db::practice::{Practice, PracticeId};
use lineup_db::rower::Rower;
use lineup_db::types::EmailLogType;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

use crate::extract::HtmlForm;
use crate::jwt::JwtKeys;
use crate::magic_link::create_magic_link;
use crate::state::{MailerCtx, TenantContext};
use crate::unsubscribe;
use crate::{
    handlers::{internal_error, ErrorResponse},
    templates,
};

/// Per-practice non-respondent info shared by preview and send.
struct ReminderRecipient {
    user_id: lineup_db::app_user::UserId,
    email: String,
    name: String,
    dates: Vec<(NaiveDate, Option<chrono::NaiveTime>)>,
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
            if user.wants_reminders() && user.status == lineup_db::app_user::UserStatus::Active {
                rower_users.insert(r.id, (user.id, user.email.clone(), r.name.clone()));
            }
        }
    }

    let mut result: Vec<ReminderRecipient> = Vec::new();
    for (rower_id, (user_id, email, name)) in &rower_users {
        let mut missing = Vec::new();
        for practice in &pending {
            let responses = Availability::map_for_practice(conn, practice.id)?;
            if !responses.contains_key(rower_id)
                && !EmailLog::already_sent_today(
                    conn,
                    team_id,
                    &EmailLogType::new("reminder"),
                    practice.date,
                )?
            {
                missing.push((practice.date, practice.time));
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
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let today = Utc::now().date_naive();
    let pids: Vec<PracticeId> = query
        .practice_ids
        .iter()
        .map(|&id| PracticeId::new(id))
        .collect();

    let recipients = tenant
        .db
        .with_conn(move |conn| gather_reminder_recipients(conn, team_id, &pids, today))
        .await
        .map_err(internal_error)?;

    let preview: Vec<templates::practices::ReminderRecipientPreview> = recipients
        .iter()
        .map(|r| templates::practices::ReminderRecipientPreview {
            name: r.name.clone(),
            dates: r.dates.iter().map(|(d, _)| *d).collect(),
        })
        .collect();

    Ok(Html(
        templates::practices::reminder_preview_modal(&preview, &query.practice_ids).into_string(),
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct SendRemindersInput {
    #[serde(default)]
    practice_ids: Vec<i32>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn send_reminders_handler(
    State(mailer_ctx): State<MailerCtx>,
    State(jwt_keys): State<JwtKeys>,
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    HtmlForm(input): HtmlForm<SendRemindersInput>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    if !tenant.config.can_send_email() {
        return Ok(Html(
            templates::practices::send_result(
                "Upgrade to unlock email. Share availability links manually.",
            )
            .into_string(),
        ));
    }
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let today = Utc::now().date_naive();
    let sender_id = tenant
        .claims
        .user_id()
        .ok_or_else(|| super::super::bad_request("Not available in superuser view."))?;
    let team_name = tenant.config.tenant_name.clone();
    let pids: Vec<PracticeId> = input
        .practice_ids
        .iter()
        .map(|&id| PracticeId::new(id))
        .collect();

    // Gather non-respondent users + record sends.
    let recipients = tenant
        .db
        .with_conn(move |conn| {
            let result = gather_reminder_recipients(conn, team_id, &pids, today)?;

            // Record sends.
            let now = Utc::now().naive_utc();
            let mut dates_sent: HashSet<NaiveDate> = HashSet::new();
            for r in &result {
                for (date, _time) in &r.dates {
                    if dates_sent.insert(*date) {
                        EmailLog::record(
                            conn,
                            NewEmailLog {
                                team_id,
                                email_type: EmailLogType::new("reminder"),
                                practice_date: *date,
                                sent_at: now,
                                recipient_count: result
                                    .iter()
                                    .filter(|r2| r2.dates.iter().any(|(d, _)| d == date))
                                    .count()
                                    as i32,
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
        let last_date = r.dates.iter().map(|(d, _)| *d).max().unwrap();
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

        let magic_url = mailer_ctx.full_url(&format!(
            "/auth/magic/{}/{raw_token}",
            tenant.config.tenant_slug
        ));
        let unsub_url = unsubscribe::url(
            &mailer_ctx,
            &tenant.config.tenant_slug,
            r.user_id,
            unsubscribe::EmailType::Reminders,
            &jwt_keys,
        );
        let unsub_all_url = unsubscribe::url(
            &mailer_ctx,
            &tenant.config.tenant_slug,
            r.user_id,
            unsubscribe::EmailType::All,
            &jwt_keys,
        );
        if let Err(err) = mailer_ctx
            .mailer
            .send_reminder(
                &r.email,
                &r.name,
                &team_name,
                &r.dates,
                &magic_url,
                &unsub_url,
                &unsub_all_url,
            )
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
            tenant.claims.audit_user_id(),
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
    Ok(Html(templates::practices::send_result(&msg).into_string()))
}
