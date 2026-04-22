//! Self-service endpoints for authenticated rowers.
//!
//! - `GET /my/profile` — view + edit own attributes
//! - `POST /my/profile` — update own attributes
//! - `GET /my/availability` — view + edit own availability
//! - `POST /my/availability` — upsert one (practice_id, status) entry

use std::collections::BTreeMap;

use axum::{http::HeaderMap, response::Html, Extension, Form};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use chrono::NaiveDate;
use lineup_db::availability::{types::AvailabilityStatus, Availability, NewAvailability};
use lineup_db::lineup::Lineup;
use lineup_db::practice::{Practice, PracticeId};
use lineup_db::rower::types::SweepBias;
use lineup_db::rower::Rower;
use serde::Deserialize;

use lineup_db::team::{SelfEditLevel, Team};

use lineup_db::app_user::AppUser;

use crate::{
    handlers::rowers::load_detail,
    handlers::{bad_request, internal_error, not_found, ErrorResponse},
    state::TenantContext,
    templates,
};

const TAB_TARGET: &str = "my-tab-content";

fn is_tab_swap(headers: &HeaderMap) -> bool {
    headers.get("HX-Target").and_then(|v| v.to_str().ok()) == Some(TAB_TARGET)
}

/// `GET /my` — default to Profile tab.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn index_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, ErrorResponse> {
    let tab_content = profile_content(&jar, &tenant).await?;
    let page = templates::my::tabbed_page("profile", tab_content);
    Ok(super::maybe_page_authed("My", page, hx, &tenant))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn profile_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    let tab_content = profile_content(&jar, &tenant).await?;
    if is_tab_swap(&headers) {
        return Ok(Html(
            templates::my::tab_content_swap("profile", tab_content).into_string(),
        ));
    }
    let page = templates::my::tabbed_page("profile", tab_content);
    Ok(super::maybe_page_authed("My", page, hx, &tenant))
}

async fn profile_content(
    jar: &CookieJar,
    tenant: &TenantContext,
) -> Result<maud::Markup, ErrorResponse> {
    let rower = match try_load_my_rower(tenant).await? {
        Some(r) => r,
        None => {
            return Ok(templates::my::no_rower_content(
                "Your account isn't linked to a roster member. Ask your coach to link your account, or sync the spreadsheet with your email address.",
            ));
        }
    };
    let rower_id = rower.id;
    let team_id = super::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let detail = load_detail(&tenant.db, rower_id).await?;
    let level = tenant
        .db
        .with_conn(move |conn| Team::get(conn, team_id))
        .await
        .map_err(internal_error)?
        .map(|t| t.self_edit_level)
        .unwrap_or(SelfEditLevel::Low);
    let perms = templates::rowers::DetailPermissions::member(level);
    Ok(templates::rowers::detail_content(&detail, perms, true))
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn profile_update_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<ProfileInput>,
) -> Result<Html<String>, ErrorResponse> {
    let mut rower = load_my_rower(&tenant).await?;

    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let level = tenant
        .db
        .with_conn(move |conn| Team::get(conn, team_id))
        .await
        .map_err(internal_error)?
        .map(|t| t.self_edit_level)
        .unwrap_or(SelfEditLevel::Low);
    let perms = templates::rowers::DetailPermissions::member(level);

    // Parse and apply — same validation as the admin rower edit,
    // but scoped to the authenticated user's own record.
    let parsed = match parse_profile(&input) {
        Ok(p) => p,
        Err(msg) => {
            let locked =
                super::rowers::locked_bucket_fields_for_rower(&tenant, &jar, &rower).await?;
            return Ok(Html(
                templates::rowers::attribute_edit_section(&rower, Some(&msg), &perms, &locked)
                    .into_string(),
            ));
        }
    };

    // Only apply fields the trust level allows.
    if perms.can_edit("weight_class") {
        rower.weight_class = parsed.weight_class;
    }
    if perms.can_edit("skill") {
        rower.skill = parsed.skill;
    }
    if perms.can_edit("strength") {
        rower.strength = parsed.strength;
    }
    if perms.can_edit("side") {
        rower.side = parsed.side;
    }
    if perms.can_edit("side_strength") {
        rower.side_strength = parsed.side_strength;
    }
    if perms.can_edit("sweep_bias") {
        rower.sweep_bias = SweepBias::new(parsed.sweep_bias);
    }

    let saved = tenant
        .db
        .with_conn(move |conn| Rower::save(conn, &rower))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "rower.update",
        "rower",
        &saved.id.to_string(),
        Some(serde_json::json!({"self_edit": true}).to_string()),
    );

    // Re-render the full detail page with correct permissions.
    let rower_id = saved.id;
    let detail = load_detail(&tenant.db, rower_id).await?;
    let content = templates::rowers::detail_content(&detail, perms, true);
    Ok(super::maybe_page_authed("My profile", content, hx, &tenant))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProfileInput {
    pub(crate) weight_class: String,
    pub(crate) skill: String,
    pub(crate) strength: String,
    pub(crate) side: String,
    pub(crate) side_strength: i32,
    #[serde(default)]
    pub(crate) sweep_bias: i32,
}

struct ParsedProfile {
    weight_class: lineup_db::rower::types::RowerWeightClass,
    skill: lineup_db::rower::types::Skill,
    strength: lineup_db::rower::types::Strength,
    side: lineup_db::rower::types::Side,
    side_strength: lineup_db::rower::types::SideStrength,
    sweep_bias: i32,
}

fn parse_profile(input: &ProfileInput) -> Result<ParsedProfile, String> {
    use lineup_db::rower::types::*;
    let weight_class = match input.weight_class.as_str() {
        "Light" => RowerWeightClass::Light,
        "Medium" => RowerWeightClass::Medium,
        "Heavy" => RowerWeightClass::Heavy,
        "VeryHeavy" => RowerWeightClass::VeryHeavy,
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
        return Err(format!(
            "side strength must be 0-5, got {}",
            input.side_strength
        ));
    }
    Ok(ParsedProfile {
        weight_class,
        skill,
        strength,
        side,
        side_strength: SideStrength::new(input.side_strength),
        sweep_bias: input.sweep_bias,
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
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    let tab_content = availability_content(&jar, &tenant).await?;
    if is_tab_swap(&headers) {
        return Ok(Html(
            templates::my::tab_content_swap("availability", tab_content).into_string(),
        ));
    }
    let page = templates::my::tabbed_page("availability", tab_content);
    Ok(super::maybe_page_authed("My", page, hx, &tenant))
}

async fn availability_content(
    jar: &CookieJar,
    tenant: &TenantContext,
) -> Result<maud::Markup, ErrorResponse> {
    let team_id = super::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let rower = match try_load_my_rower(tenant).await? {
        Some(r) => r,
        None => {
            return Ok(templates::my::no_rower_content(
                "Your account isn't linked to a roster member. Ask your coach to link your account, or sync the spreadsheet with your email address.",
            ));
        }
    };
    let rower_id = rower.id;

    let rows = tenant
        .db
        .with_conn(move |conn| {
            let today = chrono::Utc::now().date_naive();

            // Gather all relevant practices: upcoming scheduled practices.
            let practices = Practice::list_upcoming(conn, team_id, today)?;
            let mut all_practices: BTreeMap<PracticeId, (NaiveDate, Option<AvailabilityStatus>)> =
                BTreeMap::new();
            for p in &practices {
                all_practices.entry(p.id).or_insert((p.date, None));
            }

            // Load this rower's existing responses and overlay.
            let practice_ids: Vec<PracticeId> = all_practices.keys().copied().collect();
            let my_avail: Vec<Availability> = {
                use diesel::prelude::*;
                use lineup_db::schema::availability;
                availability::table
                    .filter(availability::rower_id.eq(rower_id))
                    .filter(availability::practice_id.eq_any(&practice_ids))
                    .select(Availability::as_select())
                    .get_results(conn)?
            };
            for a in &my_avail {
                if let Some(entry) = all_practices.get_mut(&a.practice_id) {
                    entry.1 = Some(a.status);
                }
            }

            // Which practices have committed lineups?
            let id_vec: Vec<PracticeId> = all_practices.keys().copied().collect();
            let committed_ids: std::collections::HashSet<PracticeId> =
                Practice::committed_ids(conn, team_id, &id_vec)?
                    .into_iter()
                    .collect();

            let rows: Vec<templates::my::AvailabilityRow> = all_practices
                .into_iter()
                .map(|(pid, (date, status))| templates::my::AvailabilityRow {
                    practice_id: pid,
                    date,
                    status,
                    has_committed: committed_ids.contains(&pid),
                })
                .collect();
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    Ok(templates::my::availability_content(&rows, None))
}

#[derive(Debug, Deserialize)]
pub(crate) struct AvailabilityInput {
    pub(crate) practice_id: PracticeId,
    pub(crate) status: String,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn availability_update_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<AvailabilityInput>,
) -> Result<Html<String>, ErrorResponse> {
    let _team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let rower = load_my_rower(&tenant).await?;

    let status = match input.status.as_str() {
        "Yes" => AvailabilityStatus::Yes,
        "No" => AvailabilityStatus::No,
        other => {
            tracing::warn!(?other, "invalid availability status");
            return Err(bad_request("Invalid availability status."));
        }
    };

    let rower_id = rower.id;
    let practice_id = input.practice_id;
    tenant
        .db
        .with_conn(move |conn| {
            Availability::upsert(
                conn,
                NewAvailability {
                    rower_id,
                    practice_id,
                    status,
                },
            )
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "availability.update",
        "availability",
        &format!("{rower_id}:{practice_id}"),
        Some(
            serde_json::json!({"status": input.status, "practice_id": practice_id.as_int()})
                .to_string(),
        ),
    );

    // Check if this change affects a committed lineup.
    let stale_warning = if !status.is_available_for_sweep() {
        let pid = practice_id;
        let rid = rower_id;
        let affected = tenant
            .db
            .with_conn(move |conn| Lineup::is_rower_in_committed_lineup(conn, pid, rid))
            .await
            .map_err(internal_error)?;
        if affected {
            // Look up the practice date for the warning message.
            let date = tenant
                .db
                .with_conn(move |conn| Practice::get(conn, pid))
                .await
                .map_err(internal_error)?
                .map(|p| p.date);
            date.map(|d| (pid, d))
        } else {
            None
        }
    } else {
        None
    };

    // Re-render the full availability page with optional warning.
    let team_id = super::active_team(&tenant.db, &jar, Some(&tenant.claims)).await?;
    let rows = tenant
        .db
        .with_conn(move |conn| {
            let today = chrono::Utc::now().date_naive();
            let practices = Practice::list_upcoming(conn, team_id, today)?;
            let mut all_practices: BTreeMap<PracticeId, (NaiveDate, Option<AvailabilityStatus>)> =
                BTreeMap::new();
            for p in &practices {
                all_practices.entry(p.id).or_insert((p.date, None));
            }
            let practice_ids: Vec<PracticeId> = all_practices.keys().copied().collect();
            let my_avail: Vec<Availability> = {
                use diesel::prelude::*;
                use lineup_db::schema::availability;
                availability::table
                    .filter(availability::rower_id.eq(rower_id))
                    .filter(availability::practice_id.eq_any(&practice_ids))
                    .select(Availability::as_select())
                    .get_results(conn)?
            };
            for a in &my_avail {
                if let Some(entry) = all_practices.get_mut(&a.practice_id) {
                    entry.1 = Some(a.status);
                }
            }
            let id_vec: Vec<PracticeId> = all_practices.keys().copied().collect();
            let committed_ids: std::collections::HashSet<PracticeId> =
                Practice::committed_ids(conn, team_id, &id_vec)?
                    .into_iter()
                    .collect();
            let rows: Vec<templates::my::AvailabilityRow> = all_practices
                .into_iter()
                .map(|(pid, (date, status))| templates::my::AvailabilityRow {
                    practice_id: pid,
                    date,
                    status,
                    has_committed: committed_ids.contains(&pid),
                })
                .collect();
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    let content = templates::my::availability_content(&rows, stale_warning);
    Ok(super::maybe_page_authed(
        "My availability",
        content,
        hx,
        &tenant,
    ))
}

// =====================================================================
// Email preferences
// =====================================================================

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn email_prefs_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    let tab_content = email_prefs_content(&tenant).await?;
    if is_tab_swap(&headers) {
        return Ok(Html(
            templates::my::tab_content_swap("email", tab_content).into_string(),
        ));
    }
    let page = templates::my::tabbed_page("email", tab_content);
    Ok(super::maybe_page_authed("My", page, hx, &tenant))
}

async fn email_prefs_content(tenant: &TenantContext) -> Result<maud::Markup, ErrorResponse> {
    let user_id = tenant
        .claims
        .user_id()
        .ok_or_else(|| super::bad_request("Not available in superuser view."))?;
    let user = tenant
        .db
        .with_conn(move |conn| AppUser::get(conn, user_id)?.ok_or(diesel::result::Error::NotFound))
        .await
        .map_err(internal_error)?;
    Ok(templates::my::email_prefs_content(&user))
}

#[derive(Debug, Deserialize)]
pub(crate) struct EmailPrefsInput {
    #[serde(default)]
    pub(crate) opt_in_reminders: Option<String>,
    #[serde(default)]
    pub(crate) opt_in_lineups: Option<String>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn email_prefs_update_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<EmailPrefsInput>,
) -> Result<Html<String>, ErrorResponse> {
    let user_id = tenant
        .claims
        .user_id()
        .ok_or_else(|| super::bad_request("Not available in superuser view."))?;
    let reminders = input.opt_in_reminders.is_some();
    let lineups = input.opt_in_lineups.is_some();

    tenant
        .db
        .with_conn(move |conn| AppUser::set_email_prefs(conn, user_id, reminders, lineups))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "email_prefs.update",
        "user",
        &user_id.to_string(),
        Some(serde_json::json!({"reminders": reminders, "lineups": lineups}).to_string()),
    );

    // Re-render with updated state.
    let user = tenant
        .db
        .with_conn(move |conn| AppUser::get(conn, user_id)?.ok_or(diesel::result::Error::NotFound))
        .await
        .map_err(internal_error)?;

    let content = templates::my::email_prefs_content(&user);
    Ok(super::maybe_page_authed(
        "Email preferences",
        content,
        hx,
        &tenant,
    ))
}

// =====================================================================
// Helpers
// =====================================================================

/// Try to load the rower linked to the authenticated user. Returns
/// None if the user has no linked rower record.
async fn try_load_my_rower(tenant: &TenantContext) -> Result<Option<Rower>, ErrorResponse> {
    let user_id = tenant
        .claims
        .user_id()
        .ok_or_else(|| super::bad_request("Not available in superuser view."))?;
    tenant
        .db
        .with_conn(move |conn| {
            let user = lineup_db::app_user::AppUser::get(conn, user_id)?;
            match user.and_then(|u| u.rower_id) {
                Some(rid) => Rower::get(conn, rid),
                None => Ok(None),
            }
        })
        .await
        .map_err(internal_error)
}

/// Load the rower linked to the authenticated user. Returns 404 if
/// the user doesn't have a linked rower record.
async fn load_my_rower(tenant: &TenantContext) -> Result<Rower, ErrorResponse> {
    try_load_my_rower(tenant)
        .await?
        .ok_or_else(|| not_found("Rower not found."))
}
