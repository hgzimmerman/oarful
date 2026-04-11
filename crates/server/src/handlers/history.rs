//! `GET /history` — list of committed practices.
//! `GET /history/{date}` — detail view for one committed practice.
//! `POST /history/{date}/notes` — update practice notes.

use axum::{
    extract::Path,
    http::StatusCode,
    response::Html,
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::NaiveDate;
use lineup_db::{app_user::Role, lineup::Lineup, practice::Practice, snapshot::DbSnapshot};
use serde::Deserialize;

use crate::{handlers::internal_error, state::TenantContext, templates};

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn list_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let practices = tenant
        .db
        .with_conn(move |conn| Practice::list_committed(conn, team_id))
        .await
        .map_err(internal_error)?;

    let content = templates::history::list_content(&practices);
    Ok(super::maybe_page_authed("History", content, hx, &tenant))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn detail_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(date): Path<NaiveDate>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let (snapshot, practice, committed) = tenant
        .db
        .with_conn(move |conn| {
            let snapshot = DbSnapshot::for_team_date(conn, team_id, date)?;
            let (practice, committed) = match Practice::find_by_date(conn, team_id, date)? {
                Some(p) => {
                    let lineups = Lineup::for_practice(conn, p.id)?;
                    (Some(p), lineups)
                }
                None => (None, Vec::new()),
            };
            Ok((snapshot, practice, committed))
        })
        .await
        .map_err(internal_error)?;

    let content = templates::history::detail_content(
        &snapshot, date, practice.as_ref(), &committed, tenant.config.force_cox_stern,
    );
    Ok(super::maybe_page_authed(
        &format!("History · {date}"),
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
    Path(date): Path<NaiveDate>,
    Form(input): Form<NotesInput>,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let notes = if input.notes.trim().is_empty() {
        None
    } else {
        Some(input.notes)
    };

    let practice = tenant
        .db
        .with_conn(move |conn| {
            // Ensure the practice exists (coach may add notes before committing).
            let p = Practice::upsert_by_date(conn, team_id, date, None)?;
            Practice::update_notes(conn, team_id, p.date, notes)
        })
        .await
        .map_err(internal_error)?;

    Ok(Html(
        templates::history::notes_display(&practice, date).into_string(),
    ))
}
