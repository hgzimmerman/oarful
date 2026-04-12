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
use std::collections::{BTreeSet, HashSet};

use lineup_db::availability::Availability;
use lineup_db::practice::Practice;
use serde::Deserialize;

use axum::extract::Path;

use crate::{handlers::internal_error, state::TenantContext, templates};

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn list_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let today = Utc::now().date_naive();
    let is_coach = tenant.claims.role()
        .unwrap_or(lineup_db::app_user::Role::Member)
        .at_least(lineup_db::app_user::Role::Coach);

    let rows = tenant
        .db
        .with_conn(move |conn| {
            // Upcoming: merge availability dates + explicitly created practices.
            let avail_dates = Availability::upcoming_dates(conn, team_id, today)?;
            let practice_dates = Practice::list_upcoming(conn, team_id, today)?;
            let upcoming: BTreeSet<_> = avail_dates.into_iter().chain(practice_dates).collect();

            // Past: committed practices (newest first, but we'll sort later).
            let past_committed = Practice::list_committed(conn, team_id)?;
            let past_dates: BTreeSet<_> = past_committed
                .iter()
                .map(|p| p.date)
                .filter(|d| *d < today)
                .collect();

            // All dates, with committed + cancelled status.
            let all_dates: BTreeSet<_> = upcoming.iter().chain(past_dates.iter()).copied().collect();
            let date_vec: Vec<_> = all_dates.iter().copied().collect();
            let committed_dates: HashSet<_> = Practice::committed_dates(conn, team_id, &date_vec)?
                .into_iter()
                .collect();
            // Build a set of cancelled dates.
            let cancelled_dates: HashSet<NaiveDate> = {
                use diesel::prelude::*;
                use lineup_db::schema::practice as p;
                p::table
                    .filter(p::team_id.eq(team_id))
                    .filter(p::date.eq_any(&date_vec))
                    .filter(p::cancelled.ne(0))
                    .select(p::date)
                    .get_results::<NaiveDate>(conn)?
                    .into_iter()
                    .collect()
            };

            let mut rows = Vec::with_capacity(all_dates.len());
            for date in all_dates.iter().rev() {
                // For upcoming dates, load availability counts.
                let (yes_count, total_responses) = if *date >= today {
                    let map = Availability::map_for_team_date(conn, team_id, *date)?;
                    let yes = map.values().filter(|s| s.is_available_for_sweep()).count();
                    (yes, map.len())
                } else {
                    (0, 0)
                };
                rows.push(templates::practices::PracticeRow {
                    date: *date,
                    yes_count,
                    total_responses,
                    has_committed: committed_dates.contains(date),
                    is_upcoming: *date >= today,
                    cancelled: cancelled_dates.contains(date),
                });
            }
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    let content = templates::practices::list_content(&rows, is_coach);
    Ok(super::maybe_page_authed("Practices", content, hx, &tenant))
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

/// `POST /practices/{date}/cancel` — toggle cancelled status.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn cancel_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(date): Path<NaiveDate>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;

    tenant
        .db
        .with_conn(move |conn| {
            let practice = Practice::find_by_date(conn, team_id, date)?
                .ok_or(diesel::result::Error::NotFound)?;
            let new_cancelled = !practice.cancelled.as_bool();
            Practice::set_cancelled(conn, team_id, date, new_cancelled)
        })
        .await
        .map_err(internal_error)?;

    Ok(Redirect::to("/practices"))
}
