//! `GET /history` — list of committed practices.
//! `GET /history/{date}` — detail view for one committed practice.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Html,
};
use axum_htmx::HxRequest;
use chrono::NaiveDate;
use lineup_db::{lineup::Lineup, practice::Practice, snapshot::DbSnapshot};

use crate::{handlers::internal_error, state::AppState, templates};

pub async fn list_handler(
    State(state): State<AppState>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    // There's no direct "list all practices" helper on Practice today;
    // derive the list from recent_placements (via the snapshot API's
    // underlying query) instead. Cheap enough for a small club.
    let dates = state
        .db
        .with_conn(|conn| {
            // `recent_placements(limit=i64::MAX)` returns every
            // committed placement, newest practice first. We only
            // need the distinct dates.
            let placements = Lineup::recent_placements(conn, i64::MAX)?;
            let mut dates: Vec<NaiveDate> =
                placements.into_iter().map(|p| p.practice_date).collect();
            dates.dedup();
            Ok(dates)
        })
        .await
        .map_err(internal_error)?;

    let content = templates::history::list_content(&dates);
    Ok(super::maybe_page("History", content, hx))
}

pub async fn detail_handler(
    State(state): State<AppState>,
    Path(date): Path<NaiveDate>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let (snapshot, committed) = state
        .db
        .with_conn(move |conn| {
            let snapshot = DbSnapshot::for_date(conn, date)?;
            let committed = match Practice::find_by_date(conn, date)? {
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
