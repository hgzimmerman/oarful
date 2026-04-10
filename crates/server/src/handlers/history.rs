//! `GET /history` — list of committed practices.
//! `GET /history/{date}` — detail view for one committed practice.

use axum::{
    extract::Path,
    http::StatusCode,
    response::Html,
    Extension,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::NaiveDate;
use lineup_db::{lineup::Lineup, practice::Practice, snapshot::DbSnapshot};

use crate::{handlers::internal_error, state::TenantContext, templates};

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
    Ok(super::maybe_page("History", content, hx))
}

pub(crate) async fn detail_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(date): Path<NaiveDate>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let (snapshot, committed) = tenant
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
