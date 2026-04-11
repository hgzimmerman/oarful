//! Self-service endpoints for authenticated rowers.
//!
//! - `GET /my/profile` — view + edit own attributes
//! - `POST /my/profile` — update own attributes
//! - `GET /my/availability` — view + edit own availability
//! - `POST /my/availability` — upsert one (date, status) entry

use std::collections::BTreeMap;

use axum::{http::StatusCode, response::Html, Extension, Form};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::NaiveDate;
use lineup_db::availability::{types::AvailabilityStatus, Availability, NewAvailability};
use lineup_db::practice::Practice;
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
    let rower = match try_load_my_rower(&tenant).await? {
        Some(r) => r,
        None => {
            let content = templates::my::no_rower_content(
                "My profile",
                "Your account isn't linked to a roster member. Ask your coach to link your account, or sync the spreadsheet with your email address.",
            );
            return Ok(super::maybe_page("My profile", content, hx));
        }
    };
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
    let rower = match try_load_my_rower(&tenant).await? {
        Some(r) => r,
        None => {
            let content = templates::my::no_rower_content(
                "My availability",
                "Your account isn't linked to a roster member. Ask your coach to link your account, or sync the spreadsheet with your email address.",
            );
            return Ok(super::maybe_page("My availability", content, hx));
        }
    };
    let rower_id = rower.id;

    let rows = tenant
        .db
        .with_conn(move |conn| {
            let today = chrono::Utc::now().date_naive();

            // Gather all relevant dates: scheduled practices + dates
            // that already have availability records for anyone.
            let practice_dates = Practice::list_upcoming(conn, team_id, today)?;
            let avail_dates = Availability::upcoming_dates(conn, team_id, today)?;
            let mut all_dates: BTreeMap<NaiveDate, Option<AvailabilityStatus>> = BTreeMap::new();
            for d in practice_dates.into_iter().chain(avail_dates) {
                all_dates.entry(d).or_insert(None);
            }

            // Load this rower's existing responses and overlay.
            use diesel::prelude::*;
            use lineup_db::schema::availability;
            let my_avail: Vec<Availability> = availability::table
                .filter(availability::rower_id.eq(rower_id))
                .filter(availability::team_id.eq(team_id))
                .filter(availability::date.ge(today))
                .select(Availability::as_select())
                .get_results(conn)?;
            for a in &my_avail {
                all_dates.insert(a.date, Some(a.status));
            }

            let rows: Vec<templates::my::AvailabilityRow> = all_dates
                .into_iter()
                .map(|(date, status)| templates::my::AvailabilityRow { date, status })
                .collect();
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    let content = templates::my::availability_content(&rower, &rows);
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

/// Try to load the rower linked to the authenticated user. Returns
/// None if the user has no linked rower record.
async fn try_load_my_rower(tenant: &TenantContext) -> Result<Option<Rower>, StatusCode> {
    let user_id = tenant.claims.sub;
    tenant
        .db
        .with_conn(move |conn| Rower::find_by_user_id(conn, user_id))
        .await
        .map_err(internal_error)
}

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
