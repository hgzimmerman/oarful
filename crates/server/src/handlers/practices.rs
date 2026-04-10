//! `GET /practices` — dashboard of upcoming practice dates with rower
//! availability counts, linking into the solve view for each date.

use axum::{extract::State, http::StatusCode, response::Html};
use axum_htmx::HxRequest;
use chrono::Utc;
use lineup_db::availability::Availability;

use crate::{handlers::internal_error, state::AppState, templates};

pub(crate) async fn list_handler(
    State(state): State<AppState>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let today = Utc::now().date_naive();
    let summaries = state
        .db
        .with_conn(move |conn| {
            let dates = Availability::upcoming_dates(conn, today)?;
            let mut rows = Vec::with_capacity(dates.len());
            for date in dates {
                let map = Availability::map_for_date(conn, date)?;
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
