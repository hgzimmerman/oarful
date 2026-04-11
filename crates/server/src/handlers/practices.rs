//! `GET /practices` — dashboard of upcoming practice dates with rower
//! availability counts, linking into the solve view for each date.

use axum::{http::StatusCode, response::Html, Extension};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::Utc;
use lineup_db::availability::Availability;

use crate::{handlers::internal_error, state::TenantContext, templates};

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn list_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let today = Utc::now().date_naive();
    let summaries = tenant
        .db
        .with_conn(move |conn| {
            let dates = Availability::upcoming_dates(conn, team_id, today)?;
            let mut rows = Vec::with_capacity(dates.len());
            for date in dates {
                let map = Availability::map_for_team_date(conn, team_id, date)?;
                let yes_count = map
                    .values()
                    .filter(|s| s.is_available_for_sweep())
                    .count();
                rows.push(templates::practices::PracticeRow {
                    date,
                    yes_count,
                    total_responses: map.len(),
                });
            }
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    let content = templates::practices::list_content(&summaries);
    Ok(super::maybe_page("Practices", content, hx))
}
