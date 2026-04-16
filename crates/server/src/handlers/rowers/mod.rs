//! Rower roster list + batch invite.
//!
//! Per-rower detail page, attribute editing, and affinity CRUD live
//! in [`detail`].

mod detail;

pub(crate) use detail::{
    attributes_handler, detail_handler, edit_attributes_handler, load_detail,
    pair_affinity_delete_handler, pair_affinity_upsert_handler, seat_affinity_delete_handler,
    seat_affinity_upsert_handler, toggle_active_handler, update_handler, RowerDetail,
};

use axum::{extract::State, response::Html, Extension};
use axum_extra::extract::CookieJar;
use lineup_db::app_user::{AppUser, Role};
use lineup_db::rower::Rower;

use crate::{
    handlers::{internal_error, ErrorResponse},
    state::{MailerCtx, TenantContext},
    templates,
};

/// A rower paired with their linked email (if any).
pub(crate) struct RosterRow {
    pub(crate) rower: Rower,
    pub(crate) email: Option<String>,
}

/// Build the roster list markup (shared by `/rowers` and `/team/roster`).
pub(crate) async fn roster_content(
    jar: &CookieJar,
    tenant: &TenantContext,
) -> Result<maud::Markup, ErrorResponse> {
    let team_id = crate::handlers::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let is_coach = tenant
        .claims
        .role()
        .unwrap_or(Role::Member)
        .at_least(Role::Coach);
    let show_emails = tenant.show_emails();
    let rows = tenant
        .db
        .with_conn(move |conn| {
            let team_rower_ids =
                lineup_db::team::TeamMembership::rower_ids_for_team(conn, team_id)?;
            let all_active = Rower::list_active(conn)?;
            let filtered: Vec<Rower> = all_active
                .into_iter()
                .filter(|r| team_rower_ids.contains(&r.id))
                .filter(|r| match AppUser::find_by_rower_id(conn, r.id) {
                    Ok(Some(u)) => {
                        u.parsed_status() != Some(lineup_db::app_user::UserStatus::Disabled)
                    }
                    _ => true,
                })
                .collect();
            let mut rows = Vec::with_capacity(filtered.len());
            for r in filtered {
                let email = if show_emails {
                    AppUser::find_by_rower_id(conn, r.id)
                        .ok()
                        .flatten()
                        .map(|u| u.email)
                } else {
                    None
                };
                rows.push(RosterRow { rower: r, email });
            }
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;
    Ok(templates::rowers::list_content(
        &rows,
        is_coach,
        show_emails,
    ))
}

/// `POST /team/roster/batch-invite` — create accounts + send invite
/// emails for all roster members with an email but no linked user.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn batch_invite_handler(
    State(mailer): State<MailerCtx>,
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = crate::handlers::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let result = tenant
        .db
        .with_conn(move |conn| {
            let team_rower_ids =
                lineup_db::team::TeamMembership::rower_ids_for_team(conn, team_id)?;
            let all_active = Rower::list_active(conn)?;
            let team_rowers: Vec<Rower> = all_active
                .into_iter()
                .filter(|r| team_rower_ids.contains(&r.id))
                .collect();

            let invited: Vec<(String, String, String)> = Vec::new();
            let mut skipped_no_email: usize = 0;
            let mut skipped_existing: usize = 0;

            for r in &team_rowers {
                if let Some(user) = AppUser::find_by_rower_id(conn, r.id)? {
                    if user.status == "active" {
                        continue;
                    }
                    skipped_existing += 1;
                    continue;
                }
                skipped_no_email += 1;
            }

            Ok((invited, skipped_no_email, skipped_existing))
        })
        .await
        .map_err(internal_error)?;

    let (invited, skipped_no_email, skipped_existing) = result;
    let invited_count = invited.len();

    for (email, name, token) in &invited {
        let invite_path = format!("/invite/{token}");
        let invite_url = mailer.full_url(&invite_path);
        if let Err(err) = mailer.mailer.send_invite(email, name, &invite_url).await {
            tracing::warn!(?err, %email, "batch invite: mailer failed");
        }
    }

    let detail = serde_json::json!({
        "invited": invited_count,
        "skipped_no_email": skipped_no_email,
        "skipped_existing": skipped_existing,
    });
    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "invite.batch",
        "team",
        &team_id.to_string(),
        Some(detail.to_string()),
    );

    let mut parts = Vec::new();
    if invited_count > 0 {
        parts.push(format!("Invited {invited_count} rower(s)."));
    }
    if skipped_no_email > 0 {
        parts.push(format!("{skipped_no_email} skipped (no email)."));
    }
    if skipped_existing > 0 {
        parts.push(format!("{skipped_existing} skipped (account exists)."));
    }
    if parts.is_empty() {
        parts.push("No rowers to invite.".to_string());
    }
    let msg = parts.join(" ");

    let is_coach = tenant
        .claims
        .role()
        .unwrap_or(Role::Member)
        .at_least(Role::Coach);
    let show_emails = tenant.show_emails();
    let rows = tenant
        .db
        .with_conn(move |conn| {
            let team_rower_ids =
                lineup_db::team::TeamMembership::rower_ids_for_team(conn, team_id)?;
            let all_active = Rower::list_active(conn)?;
            let rows: Vec<RosterRow> = all_active
                .into_iter()
                .filter(|r| team_rower_ids.contains(&r.id))
                .filter(|r| match AppUser::find_by_rower_id(conn, r.id) {
                    Ok(Some(u)) => {
                        u.parsed_status() != Some(lineup_db::app_user::UserStatus::Disabled)
                    }
                    _ => true,
                })
                .map(|r| RosterRow {
                    rower: r,
                    email: None,
                })
                .collect();
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    Ok(Html(
        templates::rowers::batch_invite_result(&msg, &rows, is_coach, show_emails).into_string(),
    ))
}
