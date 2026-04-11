//! `GET /practices` — dashboard of upcoming practice dates with rower
//! availability counts, linking into the solve view for each date.
//! `POST /practices` — create a practice for a given date.

use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::{NaiveDate, Utc};
use lineup_db::app_user::Role;
use std::collections::BTreeSet;

use lineup_db::availability::Availability;
use lineup_db::lineup::Lineup;
use lineup_db::practice::Practice;
use serde::Deserialize;

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
            // Merge dates from availability records and explicitly created practices.
            let avail_dates = Availability::upcoming_dates(conn, team_id, today)?;
            let practice_dates = Practice::list_upcoming(conn, team_id, today)?;
            let all_dates: BTreeSet<_> = avail_dates.into_iter().chain(practice_dates).collect();

            let mut rows = Vec::with_capacity(all_dates.len());
            for date in all_dates {
                let map = Availability::map_for_team_date(conn, team_id, date)?;
                let yes_count = map
                    .values()
                    .filter(|s| s.is_available_for_sweep())
                    .count();
                let has_committed = Practice::find_by_date(conn, team_id, date)?
                    .map(|p| {
                        Lineup::for_practice(conn, p.id)
                            .map(|l| !l.is_empty())
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                rows.push(templates::practices::PracticeRow {
                    date,
                    yes_count,
                    total_responses: map.len(),
                    has_committed,
                });
            }
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    let content = templates::practices::list_content(&summaries);
    Ok(super::maybe_page("Practices", content, hx))
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreatePracticeInput {
    date: String,
}

/// `POST /practices` — create a practice for a given date and redirect
/// back to the practices list.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn create_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Form(input): Form<CreatePracticeInput>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    let date = NaiveDate::parse_from_str(input.date.trim(), "%Y-%m-%d")
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    tenant
        .db
        .with_conn(move |conn| Practice::upsert_by_date(conn, team_id, date, None))
        .await
        .map_err(internal_error)?;

    Ok(Redirect::to("/practices"))
}
