//! `GET /history` — list of committed practices.
//! `GET /history/{id}` — detail view for one committed practice.
//! `POST /history/{id}/notes` — update practice notes.

use axum::{
    extract::Path,
    http::StatusCode,
    response::Html,
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use lineup_db::{app_user::Role, availability::Availability, lineup::Lineup, practice::{Practice, PracticeId}, snapshot::DbSnapshot};
use std::collections::HashSet;
use serde::Deserialize;

use crate::{handlers::internal_error, state::TenantContext, templates};

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn list_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let (practices, stale_ids) = tenant
        .db
        .with_conn(move |conn| {
            let practices = Practice::list_committed(conn, team_id)?;
            let pids: Vec<PracticeId> = practices.iter().map(|p| p.id).collect();

            // For each committed practice, check if any placed rower is no
            // longer available — that makes the lineup "stale".
            let committed_rowers = Lineup::committed_rower_ids_for_practices(conn, &pids)?;
            let avail = Availability::map_for_practices(conn, &pids)?;

            let stale_ids: HashSet<PracticeId> = committed_rowers
                .iter()
                .filter(|(pid, rower_ids)| {
                    rower_ids.iter().any(|rid| {
                        !avail
                            .get(&(*rid, **pid))
                            .map(|s| s.is_available_for_sweep())
                            .unwrap_or(false)
                    })
                })
                .map(|(pid, _)| *pid)
                .collect();

            Ok((practices, stale_ids))
        })
        .await
        .map_err(internal_error)?;

    let content = templates::history::list_content(&practices, &stale_ids);
    Ok(super::maybe_page_authed("Lineups", content, hx, &tenant))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn detail_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let _team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let (snapshot, practice, committed) = tenant
        .db
        .with_conn(move |conn| {
            let practice = Practice::get(conn, practice_id)?
                .ok_or(diesel::result::Error::NotFound)?;
            let snapshot = DbSnapshot::for_practice(conn, &practice)?;
            let lineups = Lineup::for_practice(conn, practice.id)?;
            Ok((snapshot, practice, lineups))
        })
        .await
        .map_err(internal_error)?;

    let date = practice.date;
    let is_coach = tenant.claims.role()
        .unwrap_or(lineup_db::app_user::Role::Member)
        .at_least(lineup_db::app_user::Role::Coach);
    let content = templates::history::detail_content(
        &snapshot, practice_id, date, Some(&practice), &committed, tenant.config.force_cox_stern,
        is_coach,
    );
    Ok(super::maybe_page_authed(
        &format!("Lineups · {date}"),
        content,
        hx,
        &tenant,
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct NotesInput {
    pub(crate) notes: String,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn notes_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<NotesInput>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let _team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let notes = if input.notes.trim().is_empty() {
        None
    } else {
        Some(input.notes)
    };

    let practice = tenant
        .db
        .with_conn(move |conn| {
            Practice::update_notes_by_id(conn, practice_id, notes)
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "practice.notes.update",
        "practice",
        &practice_id.to_string(),
        None,
    );

    Ok(Html(
        templates::history::notes_display(&practice).into_string(),
    ))
}
