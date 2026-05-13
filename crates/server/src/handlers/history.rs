//! `GET /practices/{id}/detail` — detail view for one committed practice.

use axum::{extract::Path, response::Html, Extension};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use lineup_db::{
    lineup::Lineup,
    practice::{Practice, PracticeId},
    snapshot::DbSnapshot,
};

use crate::{
    handlers::{internal_error, ErrorResponse},
    state::TenantContext,
    templates,
};

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn detail_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
    hx: HxRequest,
    axum::extract::Query(query): axum::extract::Query<super::timeline::EditorQuery>,
) -> Result<Html<String>, ErrorResponse> {
    let _team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let (snapshot, practice, committed, oar_assignments) = tenant
        .db
        .with_conn(move |conn| {
            let practice =
                Practice::get(conn, practice_id)?.ok_or(diesel::result::Error::NotFound)?;
            let snapshot = DbSnapshot::for_practice(conn, &practice)?;
            let lineups = Lineup::for_practice(conn, practice.id)?;
            let oar_assignments =
                lineup_db::oar_set::PracticeBoatOars::list_for_practice_with_names(
                    conn,
                    practice.id,
                )?;
            Ok((snapshot, practice, lineups, oar_assignments))
        })
        .await
        .map_err(internal_error)?;

    let date = practice.date;
    let is_coach = tenant
        .claims
        .role()
        .at_least(lineup_db::app_user::Role::Coach);
    let content = templates::history::detail_content(
        &snapshot,
        practice_id,
        date,
        Some(&practice),
        &committed,
        tenant.config.force_cox_stern,
        is_coach,
        &oar_assignments,
        query.state(),
    );
    Ok(super::maybe_page_authed(
        &format!("Lineups · {date}"),
        content,
        hx,
        &tenant,
    ))
}
