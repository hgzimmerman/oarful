//! `GET /history` — list of committed practices.
//! `GET /history/{date}` — detail view for one committed practice.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Html,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::NaiveDate;
use lineup_db::{lineup::Lineup, practice::Practice, snapshot::DbSnapshot};

use crate::{handlers::internal_error, state::AppState, templates};

pub(crate) async fn list_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let team_id = super::active_team(&state, &jar).await?;
    let practices = state
        .db
        .with_conn(move |conn| Practice::list_committed(conn, team_id))
        .await
        .map_err(internal_error)?;

    let content = templates::history::list_content(&practices);
    Ok(super::maybe_page("History", content, hx))
}

pub(crate) async fn detail_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(date): Path<NaiveDate>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let team_id = super::active_team(&state, &jar).await?;
    let (snapshot, committed) = state
        .db
        .with_conn(move |conn| {
            let snapshot = DbSnapshot::for_team_date(conn, team_id, date)?;
            let committed = match Practice::find_by_date(conn, team_id, date)? {
                Some(p) => Lineup::for_practice(conn, p.id)?,
                None => Vec::new(),
            };
            Ok((snapshot, committed))
        })
        .await
        .map_err(internal_error)?;

    let content = templates::history::detail_content(&snapshot, date, &committed);
    Ok(super::maybe_page(
        &format!("History · {date}"),
        content,
        hx,
    ))
}
