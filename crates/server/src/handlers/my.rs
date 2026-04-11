//! Self-service endpoints for authenticated rowers.
//!
//! - `GET /my/profile` — view + edit own attributes
//! - `POST /my/profile` — update own attributes
//! - `GET /my/availability` — view + edit own availability
//! - `POST /my/availability` — upsert one (date, status) entry

use axum::{http::StatusCode, response::Html, Extension, Form};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::NaiveDate;
use lineup_db::availability::{types::AvailabilityStatus, Availability, NewAvailability};
use lineup_db::rower::Rower;
use lineup_db::types::IntBool;
use serde::Deserialize;

use crate::{handlers::internal_error, state::TenantContext, templates};

// =====================================================================
// Profile
// =====================================================================

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn profile_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let rower = load_my_rower(&tenant).await?;
    let content = templates::my::profile_content(&rower);
    Ok(super::maybe_page("My profile", content, hx))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn profile_update_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<ProfileInput>,
) -> Result<Html<String>, StatusCode> {
    let mut rower = load_my_rower(&tenant).await?;

    // Parse and apply — same validation as the admin rower edit,
    // but scoped to the authenticated user's own record.
    let parsed = match parse_profile(&input) {
        Ok(p) => p,
        Err(msg) => {
            let content = templates::my::profile_content_with_error(&rower, &msg);
            return Ok(super::maybe_page("My profile", content, hx));
        }
    };

    rower.weight_class = parsed.weight_class;
    rower.skill = parsed.skill;
    rower.strength = parsed.strength;
    rower.side = parsed.side;
    rower.side_strength = parsed.side_strength;
    rower.can_scull = IntBool::new(parsed.can_scull);

    let saved = tenant
        .db
        .with_conn(move |conn| Rower::save(conn, &rower))
        .await
        .map_err(internal_error)?;

    let content = templates::my::profile_content(&saved);
    Ok(super::maybe_page("My profile", content, hx))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProfileInput {
    pub(crate) weight_class: String,
    pub(crate) skill: String,
    pub(crate) strength: String,
    pub(crate) side: String,
    pub(crate) side_strength: i32,
    #[serde(default)]
    pub(crate) can_scull: Option<String>,
}

struct ParsedProfile {
    weight_class: lineup_db::rower::types::RowerWeightClass,
    skill: lineup_db::rower::types::Skill,
    strength: lineup_db::rower::types::Strength,
    side: lineup_db::rower::types::Side,
    side_strength: lineup_db::rower::types::SideStrength,
    can_scull: bool,
}

fn parse_profile(input: &ProfileInput) -> Result<ParsedProfile, String> {
    use lineup_db::rower::types::*;
    let weight_class = match input.weight_class.as_str() {
        "Light" => RowerWeightClass::Light,
        "Medium" => RowerWeightClass::Medium,
        "Heavy" => RowerWeightClass::Heavy,
        other => return Err(format!("invalid weight class: {other}")),
    };
    let skill = match input.skill.as_str() {
        "Novice" => Skill::Novice,
        "Intermediate" => Skill::Intermediate,
        "Master" => Skill::Master,
        "Expert" => Skill::Expert,
        other => return Err(format!("invalid skill: {other}")),
    };
    let strength = match input.strength.as_str() {
        "Weak" => Strength::Weak,
        "Intermediate" => Strength::Intermediate,
        "Strong" => Strength::Strong,
        "VeryStrong" => Strength::VeryStrong,
        other => return Err(format!("invalid strength: {other}")),
    };
    let side = match input.side.as_str() {
        "Port" => Side::Port,
        "Starboard" => Side::Starboard,
        "Either" => Side::Either,
        other => return Err(format!("invalid side: {other}")),
    };
    if !(0..=5).contains(&input.side_strength) {
        return Err(format!("side strength must be 0-5, got {}", input.side_strength));
    }
    Ok(ParsedProfile {
        weight_class,
        skill,
        strength,
        side,
        side_strength: SideStrength::new(input.side_strength),
        can_scull: input.can_scull.is_some(),
    })
}

// =====================================================================
// Availability
// =====================================================================

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn availability_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let rower = load_my_rower(&tenant).await?;

    let entries = tenant
        .db
        .with_conn(move |conn| {
            // Show existing entries + upcoming dates with no response.
            let today = chrono::Utc::now().date_naive();
            let dates = Availability::upcoming_dates(conn, team_id, today)?;
            let map = Availability::map_for_team_date(conn, team_id, today)?;
            // Include dates from the map that are for this rower.
            let all_avail = dates
                .into_iter()
                .map(|date| {
                    let status = map.get(&rower.id).copied();
                    // Re-query per date for this specific rower.
                    let _ = status; // We need per-date, not per-rower.
                    (date, None::<AvailabilityStatus>)
                })
                .collect::<Vec<_>>();
            // Actually, load all availability for this rower on this team.
            use lineup_db::schema::availability;
            use diesel::prelude::*;
            let rower_avail: Vec<Availability> = availability::table
                .filter(availability::rower_id.eq(rower.id))
                .filter(availability::team_id.eq(team_id))
                .filter(availability::date.ge(today))
                .order(availability::date.asc())
                .select(Availability::as_select())
                .get_results(conn)?;
            let _ = all_avail;
            Ok(rower_avail)
        })
        .await
        .map_err(internal_error)?;

    let content = templates::my::availability_content(&rower, &entries);
    Ok(super::maybe_page("My availability", content, hx))
}

#[derive(Debug, Deserialize)]
pub(crate) struct AvailabilityInput {
    pub(crate) date: NaiveDate,
    pub(crate) status: String,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn availability_update_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<AvailabilityInput>,
) -> Result<Html<String>, StatusCode> {
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let rower = load_my_rower(&tenant).await?;

    let status = match input.status.as_str() {
        "Yes" => AvailabilityStatus::Yes,
        "No" => AvailabilityStatus::No,
        "Maybe" => AvailabilityStatus::Maybe,
        "ScullingOnly" => AvailabilityStatus::ScullingOnly,
        other => {
            tracing::warn!(?other, "invalid availability status");
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let rower_id = rower.id;
    let date = input.date;
    tenant
        .db
        .with_conn(move |conn| {
            Availability::upsert(
                conn,
                NewAvailability {
                    rower_id,
                    team_id,
                    date,
                    status,
                },
            )
        })
        .await
        .map_err(internal_error)?;

    // Re-render the full availability page.
    availability_handler(jar, Extension(tenant), hx).await
}

// =====================================================================
// Helpers
// =====================================================================

/// Load the rower linked to the authenticated user. Returns 404 if
/// the user doesn't have a linked rower record.
async fn load_my_rower(tenant: &TenantContext) -> Result<Rower, StatusCode> {
    let user_id = tenant.claims.sub;
    let maybe = tenant
        .db
        .with_conn(move |conn| Rower::find_by_user_id(conn, user_id))
        .await
        .map_err(internal_error)?;
    maybe.ok_or(StatusCode::NOT_FOUND)
}
