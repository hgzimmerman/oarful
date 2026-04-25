//! Lineup preview + send handlers.

use axum::{extract::State, response::Html, Extension};
use axum_extra::extract::CookieJar;
use chrono::NaiveDate;
use lineup_db::app_user::{AppUser, Role};
use lineup_db::availability::Availability;
use lineup_db::boat::Boat;
use lineup_db::email_log::{EmailLog, NewEmailLog};
use lineup_db::lineup::Lineup;
use lineup_db::practice::Practice;
use lineup_db::rower::Rower;
use lineup_db::types::EmailLogType;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

use crate::extract::HtmlForm;
use crate::jwt::JwtKeys;
use crate::magic_link::create_magic_link;
use crate::mailer::{EmailBoatLineup, EmailLineupSummary, EmailSeat};
use crate::state::{MailerCtx, TenantContext};
use crate::unsubscribe;
use crate::{
    handlers::{internal_error, ErrorResponse},
    templates,
};

#[derive(Debug, Deserialize)]
pub(crate) struct SendLineupsInput {
    #[serde(default)]
    pub(crate) dates: Vec<String>,
    #[serde(default = "default_scope")]
    pub(crate) scope: String,
}

fn default_scope() -> String {
    "placed".to_string()
}

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
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let scope_all = query.scope == "all";

    let dates: Vec<NaiveDate> = query
        .dates
        .iter()
        .filter_map(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
        .collect();

    let date_strs: Vec<String> = dates
        .iter()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .collect();

    if dates.is_empty() {
        return Ok(Html(
            templates::practices::lineup_preview_modal(&[], &date_strs, "placed").into_string(),
        ));
    }

    let scope = query.scope.clone();
    let recipients = tenant
        .db
        .with_conn(move |conn| gather_lineup_recipients(conn, team_id, &dates, scope_all))
        .await
        .map_err(internal_error)?;

    let preview: Vec<templates::practices::LineupRecipientPreview> = recipients
        .iter()
        .map(
            |(_uid, _email, name)| templates::practices::LineupRecipientPreview {
                name: name.clone(),
            },
        )
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
        if EmailLog::already_sent_today(conn, team_id, &EmailLogType::new("lineup"), *date)? {
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
        valid_dates.push(*date);

        for cl in &committed {
            for seat_row in &cl.seats {
                placed_rower_ids.insert(seat_row.rower_id);
            }
        }

        let available = Availability::map_for_practice(conn, practice.id)?;
        for (rid, status) in &available {
            if status.is_available() && !placed_rower_ids.contains(rid) {
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
                if user.wants_lineups() && user.status == lineup_db::app_user::UserStatus::Active {
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
    State(mailer_ctx): State<MailerCtx>,
    State(jwt_keys): State<JwtKeys>,
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    HtmlForm(input): HtmlForm<SendLineupsInput>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    if !tenant.config.can_send_email() {
        return Ok(Html(
            templates::practices::send_result_billing_gate("Upgrade to unlock email.")
                .into_string(),
        ));
    }
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let sender_id = tenant
        .claims
        .user_id()
        .ok_or_else(|| super::super::bad_request("Not available in superuser view."))?;
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
            templates::practices::send_result_billing_gate(
                "No practices selected — check at least one to send lineups.",
            )
            .into_string(),
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
            let mut first_practice_id: Option<lineup_db::practice::PracticeId> = None;
            let mut placed_rower_ids: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();
            let mut benched_rower_ids: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();

            for date in &dates {
                if EmailLog::already_sent_today(conn, team_id, &EmailLogType::new("lineup"), *date)?
                {
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

                let mut boats = Vec::new();
                let mut date_placed: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();
                for cl in &committed {
                    let boat = Boat::get(conn, cl.lineup.boat_id)?;
                    let boat_name = boat
                        .map(|b| b.name.clone())
                        .unwrap_or_else(|| "Unknown".to_string());

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
                        seats.push(EmailSeat { label, rower_name });
                        date_placed.insert(seat_row.rower_id);
                    }

                    boats.push(EmailBoatLineup { boat_name, seats });
                }

                let available = Availability::map_for_practice(conn, practice.id)?;
                let mut benched_names = Vec::new();
                let mut date_benched: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();
                for (rid, status) in &available {
                    if status.is_available() && !date_placed.contains(rid) {
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
                    time: practice.time,
                    boats,
                    benched: benched_names,
                });
            }

            if summaries.is_empty() {
                return Ok((summaries, Vec::new(), first_practice_id));
            }

            let mut recipient_rower_ids: HashSet<lineup_db::rower::types::RowerId> = HashSet::new();
            recipient_rower_ids.extend(&placed_rower_ids);
            recipient_rower_ids.extend(&benched_rower_ids);

            if scope_all {
                for date in &dates {
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

            let mut recipients: Vec<(lineup_db::app_user::UserId, String, String)> = Vec::new();
            for rid in &recipient_rower_ids {
                if let Some(r) = rower_map.get(rid) {
                    if let Some(user) = AppUser::find_by_rower_id(conn, r.id)? {
                        if user.wants_lineups()
                            && user.status == lineup_db::app_user::UserStatus::Active
                        {
                            recipients.push((user.id, user.email.clone(), r.name.clone()));
                        }
                    }
                }
            }

            // Record sends.
            let now = chrono::Utc::now().naive_utc();
            for summary in &summaries {
                EmailLog::record(
                    conn,
                    NewEmailLog {
                        team_id,
                        email_type: EmailLogType::new("lineup"),
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
    let mut results: Vec<templates::practices::SendResultRecipient> = Vec::new();
    if !summaries.is_empty() {
        let last_date = summaries.iter().map(|s| s.date).max().unwrap();
        let expires_at = last_date.and_hms_opt(23, 59, 59).unwrap();

        for (user_id, email, name) in &recipients {
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

            let magic_url = mailer_ctx.full_url(&format!(
                "/auth/magic/{}/{raw_token}",
                tenant.config.tenant_slug
            ));
            let unsub_url = unsubscribe::url(
                &mailer_ctx,
                &tenant.config.tenant_slug,
                *user_id,
                unsubscribe::EmailType::Lineups,
                &jwt_keys,
            );
            let unsub_all_url = unsubscribe::url(
                &mailer_ctx,
                &tenant.config.tenant_slug,
                *user_id,
                unsubscribe::EmailType::All,
                &jwt_keys,
            );
            if let Err(err) = mailer_ctx
                .mailer
                .send_lineup(
                    email,
                    name,
                    &team_name,
                    &summaries,
                    &magic_url,
                    &unsub_url,
                    &unsub_all_url,
                )
                .await
            {
                tracing::warn!(?err, %email, "failed to send lineup");
                results.push(templates::practices::SendResultRecipient {
                    name: name.clone(),
                    status: templates::practices::SendStatus::Failed,
                });
            } else {
                results.push(templates::practices::SendResultRecipient {
                    name: name.clone(),
                    status: templates::practices::SendStatus::Sent,
                });
            }
        }
    }

    let sent_count = results
        .iter()
        .filter(|r| matches!(r.status, templates::practices::SendStatus::Sent))
        .count();
    if sent_count > 0 {
        let date_strs: Vec<String> = summaries.iter().map(|s| s.date.to_string()).collect();
        crate::audit::record(
            &tenant.db,
            tenant.claims.audit_user_id(),
            "practices.send_lineups",
            "practice",
            &date_strs.join(","),
            Some(serde_json::json!({"sent_count": sent_count, "dates": date_strs}).to_string()),
        );
    }

    Ok(Html(
        templates::practices::send_result_modal("Lineups sent", &results).into_string(),
    ))
}
