//! Background poller that sends stale-lineup alert emails to coaches
//! when rower availability changes affect committed lineups.
//!
//! Runs every 5 minutes. Two urgency tiers:
//! - **Urgent** (practice starts within 3 hours): sent immediately
//! - **Non-urgent** (>3h or no time set): batched into 6-hour digests
//!
//! One consolidated email per coach, grouped by team.

use std::collections::{HashMap, HashSet};

use chrono::{NaiveDateTime, Utc};
use lineup_db::app_user::{AppUser, Role, UserId};
use lineup_db::availability::Availability;
use lineup_db::email_log::{EmailLog, NewEmailLog};
use lineup_db::lineup::Lineup;
use lineup_db::practice::{Practice, PracticeId};
use lineup_db::rower::types::RowerId;
use lineup_db::rower::Rower;
use lineup_db::state::Db;
use lineup_db::team::{Team, TeamId, TeamMembership};
use lineup_db::types::EmailLogType;

use axum::extract::FromRef;

use crate::mailer::{StaleAlertPractice, StaleAlertSection};

const URGENT_HOURS: i64 = 3;
const DIGEST_INTERVAL_HOURS: i64 = 6;
const STALE_ALERT_TYPE: &str = "stale_alert";
const STALE_DIGEST_TYPE: &str = "stale_digest";

/// One polling sweep across all tenants.
pub async fn poll_stale_alerts(state: &crate::AppState) {
    let tenants = match state
        .master_db
        .with_conn(lineup_master_db::tenant::Tenant::list_all)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(?e, "stale alert poll: failed to list tenants");
            return;
        }
    };

    for tenant in &tenants {
        // Skip demo tenants.
        if tenant.demo_expires_at.is_some() {
            continue;
        }
        // Skip tenants that can't send email.
        let (db, config) = match state.tenant_db(tenant.id).await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(tenant_id = %tenant.id, ?e, "stale alert: failed to open tenant DB");
                continue;
            }
        };
        if !config.can_send_email() {
            continue;
        }

        if let Err(e) = poll_tenant(state, &db, &config).await {
            tracing::warn!(tenant_id = %tenant.id, ?e, "stale alert: tenant poll failed");
        }
    }
}

/// Info about a stale practice ready for alerting.
struct StalePractice {
    practice: Practice,
    team: Team,
    urgent: bool,
    unavailable_rower_ids: Vec<RowerId>,
}

async fn poll_tenant(
    state: &crate::AppState,
    db: &Db,
    config: &crate::tenant_cache::TenantConfig,
) -> anyhow::Result<()> {
    let now = Utc::now();
    let today = now.date_naive();
    let now_naive = now.naive_utc();

    // Gather all stale practices across all teams.
    let stale_practices: Vec<StalePractice> = db
        .with_conn(move |conn| {
            let teams = Team::list_active(conn)?;
            let mut result = Vec::new();

            for team in &teams {
                let assume_available = team.assume_available.as_bool();
                let practices: Vec<Practice> = Practice::list_committed(conn, team.id)?
                    .into_iter()
                    .filter(|p| p.date >= today)
                    .collect();

                if practices.is_empty() {
                    continue;
                }

                let pids: Vec<PracticeId> = practices.iter().map(|p| p.id).collect();
                let committed_rowers = Lineup::committed_rower_ids_for_practices(conn, &pids)?;
                let avail = Availability::map_for_practices(conn, &pids)?;

                for practice in practices {
                    let Some(placed) = committed_rowers.get(&practice.id) else {
                        continue;
                    };
                    let unavailable: Vec<RowerId> = placed
                        .iter()
                        .filter(|rid| {
                            !avail
                                .get(&(**rid, practice.id))
                                .map(|s| s.is_available())
                                .unwrap_or(assume_available)
                        })
                        .copied()
                        .collect();

                    if unavailable.is_empty() {
                        continue;
                    }

                    // Classify urgency.
                    let urgent = practice.time.is_some_and(|t| {
                        let start = practice.date.and_time(t);
                        let hours_until = (start - now_naive).num_hours();
                        hours_until < URGENT_HOURS
                    });

                    result.push(StalePractice {
                        practice,
                        team: team.clone(),
                        urgent,
                        unavailable_rower_ids: unavailable,
                    });
                }
            }
            Ok(result)
        })
        .await?;

    if stale_practices.is_empty() {
        return Ok(());
    }

    // Split into urgent and non-urgent.
    let (urgent, non_urgent): (Vec<_>, Vec<_>) =
        stale_practices.into_iter().partition(|sp| sp.urgent);

    // Check digest timing for non-urgent.
    let should_send_digest = if non_urgent.is_empty() {
        false
    } else {
        let now_naive_clone = now_naive;
        db.with_conn(move |conn| {
            use diesel::prelude::*;
            use lineup_db::schema::stale_digest_log;
            let last: Option<NaiveDateTime> = stale_digest_log::table
                .select(stale_digest_log::last_sent_at)
                .order(stale_digest_log::last_sent_at.desc())
                .first(conn)
                .optional()?;
            Ok(last
                .map(|l| (now_naive_clone - l).num_hours() >= DIGEST_INTERVAL_HOURS)
                .unwrap_or(true))
        })
        .await?
    };

    // Combine practices to send.
    let mut to_send: Vec<&StalePractice> = Vec::new();

    // Urgent: dedup per-practice (only alert once per day per practice).
    for sp in &urgent {
        let team_id = sp.team.id;
        let practice_date = sp.practice.date;
        let already = db
            .with_conn(move |conn| {
                EmailLog::already_sent_today(
                    conn,
                    team_id,
                    &EmailLogType::new(STALE_ALERT_TYPE),
                    practice_date,
                )
            })
            .await?;
        if !already {
            to_send.push(sp);
        }
    }

    // Non-urgent: include all if digest window has elapsed.
    if should_send_digest {
        for sp in &non_urgent {
            to_send.push(sp);
        }
    }

    if to_send.is_empty() {
        return Ok(());
    }

    // Resolve rower names and build per-coach email data.
    let all_rower_ids: HashSet<RowerId> = to_send
        .iter()
        .flat_map(|sp| sp.unavailable_rower_ids.iter().copied())
        .collect();

    let rower_names: HashMap<RowerId, String> = db
        .with_conn(move |conn| {
            let mut map = HashMap::new();
            for rid in all_rower_ids {
                if let Some(r) = Rower::get(conn, rid)? {
                    map.insert(rid, r.name);
                }
            }
            Ok(map)
        })
        .await?;

    // Group by team for email sections.
    let mut sections_by_team: HashMap<TeamId, StaleAlertSection> = HashMap::new();
    let mut affected_team_ids: HashSet<TeamId> = HashSet::new();

    for sp in &to_send {
        affected_team_ids.insert(sp.team.id);
        let section = sections_by_team
            .entry(sp.team.id)
            .or_insert_with(|| StaleAlertSection {
                team_name: sp.team.name.clone(),
                practices: Vec::new(),
            });
        section.practices.push(StaleAlertPractice {
            date: sp.practice.date,
            time: sp.practice.time,
            urgent: sp.urgent,
            unavailable_rowers: sp
                .unavailable_rower_ids
                .iter()
                .filter_map(|rid| rower_names.get(rid).cloned())
                .collect(),
        });
    }

    // Find all coaches to notify (Coach+ on affected teams, plus PDs).
    let affected_teams: Vec<TeamId> = affected_team_ids.into_iter().collect();
    let coach_recipients: Vec<(UserId, Vec<TeamId>)> = db
        .with_conn(move |conn| {
            let mut user_teams: HashMap<UserId, Vec<TeamId>> = HashMap::new();

            // Coaches assigned to affected teams.
            for &tid in &affected_teams {
                let coach_ids = TeamMembership::coach_user_ids_for_team(conn, tid)?;
                for uid in coach_ids {
                    user_teams.entry(uid).or_default().push(tid);
                }
            }

            // PDs get alerts for all affected teams.
            use diesel::prelude::*;
            use lineup_db::schema::user_role;
            let pd_ids: Vec<UserId> = user_role::table
                .filter(user_role::role.eq(Role::ProgramDirector))
                .select(user_role::user_id)
                .get_results(conn)?;
            for uid in pd_ids {
                user_teams
                    .entry(uid)
                    .or_insert_with(|| affected_teams.clone());
            }

            Ok(user_teams.into_iter().collect::<Vec<_>>())
        })
        .await?;

    // Send one email per coach, filtered to their teams.
    let slug = &config.tenant_slug;
    for (user_id, team_ids) in &coach_recipients {
        let user: Option<AppUser> = db
            .with_conn({
                let uid = *user_id;
                move |conn| AppUser::get(conn, uid)
            })
            .await?;
        let Some(user) = user else { continue };
        if !user.wants_stale_alerts() {
            continue;
        }

        // Build sections for this coach's teams only.
        let coach_sections: Vec<StaleAlertSection> = team_ids
            .iter()
            .filter_map(|tid| sections_by_team.get(tid).cloned())
            .filter(|s| !s.practices.is_empty())
            .collect();

        if coach_sections.is_empty() {
            continue;
        }

        // Build subject with team names.
        let team_names: Vec<&str> = coach_sections
            .iter()
            .map(|s| s.team_name.as_str())
            .collect();
        let subject = format!("Lineup changes: {}", team_names.join(", "));

        // Build unsubscribe URLs.
        let unsub_url = crate::unsubscribe::url(
            &crate::state::MailerCtx::from_ref(state),
            slug,
            user.id,
            crate::unsubscribe::EmailType::StaleAlerts,
            &state.jwt_keys,
        );
        let unsub_all_url = crate::unsubscribe::url(
            &crate::state::MailerCtx::from_ref(state),
            slug,
            user.id,
            crate::unsubscribe::EmailType::All,
            &state.jwt_keys,
        );

        // Magic link to /practices.
        let magic_url = crate::state::MailerCtx::from_ref(state).full_url("/practices");

        if let Err(e) = state
            .mailer
            .send_stale_alert(
                &user.email,
                &user.name,
                &subject,
                &coach_sections,
                &magic_url,
                &unsub_url,
                &unsub_all_url,
            )
            .await
        {
            tracing::warn!(user_id = %user.id, ?e, "stale alert: failed to send email");
        }
    }

    // Record sends in email_log for dedup.
    let sent_at = Utc::now().naive_utc();
    let recipient_count = coach_recipients.len() as i32;
    // Use a system user ID of 0 for background jobs.
    let system_user = UserId::new(0);

    for sp in &to_send {
        let team_id = sp.team.id;
        let practice_date = sp.practice.date;
        let email_type = if sp.urgent {
            STALE_ALERT_TYPE
        } else {
            STALE_DIGEST_TYPE
        };
        let rc = recipient_count;
        let sat = sent_at;
        let et = email_type.to_string();
        db.with_conn(move |conn| {
            EmailLog::record(
                conn,
                NewEmailLog {
                    team_id,
                    email_type: lineup_db::types::EmailLogType::new(&et),
                    practice_date,
                    sent_at: sat,
                    recipient_count: rc,
                    sent_by_user_id: system_user,
                },
            )
        })
        .await?;
    }

    // Update digest timestamp if we sent non-urgent alerts.
    if should_send_digest && !non_urgent.is_empty() {
        let sat = sent_at;
        db.with_conn(move |conn| {
            use diesel::prelude::*;
            use lineup_db::schema::stale_digest_log;
            diesel::insert_into(stale_digest_log::table)
                .values(stale_digest_log::last_sent_at.eq(sat))
                .execute(conn)
        })
        .await?;
    }

    let urgent_count = urgent.len();
    let digest_count = if should_send_digest {
        non_urgent.len()
    } else {
        0
    };
    if urgent_count > 0 || digest_count > 0 {
        tracing::info!(
            urgent_count,
            digest_count,
            recipients = coach_recipients.len(),
            "stale alert: sent notifications"
        );
    }

    Ok(())
}
