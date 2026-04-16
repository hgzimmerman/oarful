//! Per-rower detail page: attributes, seat affinities, pair affinities,
//! soft-delete/reactivate.

use axum::{
    extract::Path,
    http::StatusCode,
    response::Html,
    Extension, Form,
};
use axum_extra::extract::CookieJar;
use axum_htmx::HxRequest;
use lineup_db::pair_affinity::PairAffinity;
use lineup_db::rower::{
    types::{Height, RowerId, RowerWeightClass, Side, SideStrength, Skill, Strength, SweepBias},
    Rower,
};
use lineup_db::seat_affinity::{SeatAffinity, SeatZone};
use lineup_db::state::Db;
use lineup_db::types::{AffinityWeight, IntBool, AFFINITY_WEIGHT_MAX, AFFINITY_WEIGHT_MIN};
use serde::Deserialize;

use lineup_db::app_user::{AppUser, Role};
use lineup_db::team::{SelfEditLevel, Team};

use crate::{handlers::{internal_error, ErrorResponse, not_found}, state::TenantContext, templates};

/// `GET /rowers/{id}/attributes` — read-only attribute section partial.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn attributes_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
) -> Result<Html<String>, ErrorResponse> {
    let perms = resolve_perms(&tenant, &jar, id).await?;
    let rower = load(&tenant.db, id).await?;
    Ok(Html(
        templates::rowers::attribute_section(&rower, None, &perms).into_string(),
    ))
}

/// `GET /rowers/{id}/edit-attributes` — editable attribute form partial.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn edit_attributes_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
) -> Result<Html<String>, ErrorResponse> {
    let perms = resolve_perms(&tenant, &jar, id).await?;
    let rower = load(&tenant.db, id).await?;
    Ok(Html(
        templates::rowers::attribute_edit_section(&rower, None, &perms).into_string(),
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct RowerEditInput {
    pub(crate) weight_class: String,
    pub(crate) skill: String,
    pub(crate) strength: String,
    pub(crate) height: String,
    pub(crate) side: String,
    pub(crate) side_strength: i32,
    #[serde(default)]
    pub(crate) can_cox: Option<String>,
    #[serde(default)]
    pub(crate) sweep_bias: i32,
    #[serde(default)]
    pub(crate) is_designated_cox: Option<String>,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn update_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<RowerEditInput>,
) -> Result<Html<String>, ErrorResponse> {
    let perms = resolve_perms(&tenant, &jar, id).await?;
    let mut rower = load(&tenant.db, id).await?;

    let parsed = parse_input(&input);
    let typed = match parsed {
        Ok(typed) => typed,
        Err(msg) => {
            return Ok(Html(
                templates::rowers::attribute_edit_section(&rower, Some(&msg), &perms).into_string(),
            ));
        }
    };

    if perms.can_edit("weight_class") { rower.weight_class = typed.weight_class; }
    if perms.can_edit("skill") { rower.skill = typed.skill; }
    if perms.can_edit("strength") { rower.strength = typed.strength; }
    if perms.can_edit("height") { rower.height = typed.height; }
    if perms.can_edit("side") { rower.side = typed.side; }
    if perms.can_edit("side_strength") { rower.side_strength = typed.side_strength; }
    if perms.can_edit("can_cox") { rower.can_cox = IntBool::new(typed.can_cox); }
    if perms.can_edit("sweep_bias") { rower.sweep_bias = SweepBias::new(typed.sweep_bias); }
    if perms.can_edit("is_designated_cox") { rower.is_designated_cox = IntBool::new(typed.is_designated_cox); }

    let saved = tenant
        .db
        .with_conn(move |conn| Rower::save(conn, &rower))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "rower.update",
        "rower",
        &saved.id.to_string(),
        None,
    );

    Ok(Html(
        templates::rowers::attribute_section(&saved, None, &perms).into_string(),
    ))
}

struct ParsedEdit {
    weight_class: RowerWeightClass,
    skill: Skill,
    strength: Strength,
    height: Height,
    side: Side,
    side_strength: SideStrength,
    can_cox: bool,
    sweep_bias: i32,
    is_designated_cox: bool,
}

fn parse_input(input: &RowerEditInput) -> Result<ParsedEdit, String> {
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
    let height = match input.height.as_str() {
        "Short" => Height::Short,
        "Medium" => Height::Medium,
        "Tall" => Height::Tall,
        "VeryTall" => Height::VeryTall,
        other => return Err(format!("invalid height: {other}")),
    };
    let side = match input.side.as_str() {
        "Port" => Side::Port,
        "Starboard" => Side::Starboard,
        "Either" => Side::Either,
        other => return Err(format!("invalid side: {other}")),
    };
    if !(0..=5).contains(&input.side_strength) {
        return Err(format!(
            "side strength must be between 0 and 5, got {}",
            input.side_strength
        ));
    }
    let side_strength = SideStrength::new(input.side_strength);

    Ok(ParsedEdit {
        weight_class,
        skill,
        strength,
        height,
        side,
        side_strength,
        can_cox: input.can_cox.is_some(),
        sweep_bias: input.sweep_bias,
        is_designated_cox: input.is_designated_cox.is_some(),
    })
}

async fn load(db: &Db, id: RowerId) -> Result<Rower, ErrorResponse> {
    let maybe = db
        .with_conn(move |conn| Rower::get(conn, id))
        .await
        .map_err(internal_error)?;
    maybe.ok_or_else(|| not_found("Rower not found."))
}

async fn resolve_perms(
    tenant: &TenantContext,
    jar: &CookieJar,
    rower_id: RowerId,
) -> Result<templates::rowers::DetailPermissions, ErrorResponse> {
    let is_coach = tenant.claims.role().unwrap_or(Role::Member).at_least(Role::Coach);
    if is_coach {
        return Ok(templates::rowers::DetailPermissions::coach());
    }
    let uid = tenant.claims.user_id();
    let owns_rower = tenant
        .db
        .with_conn(move |conn| {
            let user = AppUser::get(conn, uid)?;
            Ok(user.and_then(|u| u.rower_id) == Some(rower_id))
        })
        .await
        .map_err(internal_error)?;
    if !owns_rower {
        return Err(crate::handlers::ErrorResponse(StatusCode::FORBIDDEN, "You don't have permission to perform this action.".into()));
    }
    let team_id = crate::handlers::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let level = tenant
        .db
        .with_conn(move |conn| Team::get(conn, team_id))
        .await
        .map_err(internal_error)?
        .map(|t| SelfEditLevel::from_str(&t.self_edit_level))
        .unwrap_or(SelfEditLevel::Low);
    Ok(templates::rowers::DetailPermissions::member(level))
}

/// `GET /rowers/{id}` — full detail page.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn detail_handler(
    jar: CookieJar,
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    hx: HxRequest,
) -> Result<Html<String>, ErrorResponse> {
    let perms = resolve_perms(&tenant, &jar, id).await?;
    let detail = load_detail(&tenant.db, id).await?;
    let content = templates::rowers::detail_content(&detail, perms);
    Ok(crate::handlers::maybe_page_authed(
        &format!("Rower · {}", detail.rower.name),
        content,
        hx,
        &tenant,
    ))
}

pub(crate) struct RowerDetail {
    pub(crate) rower: Rower,
    pub(crate) seat_affinities: Vec<SeatAffinity>,
    pub(crate) pair_affinities: Vec<PairAffinity>,
    pub(crate) other_rowers: Vec<Rower>,
}

pub(crate) async fn load_detail(db: &Db, id: RowerId) -> Result<RowerDetail, ErrorResponse> {
    let maybe = db
        .with_conn(move |conn| {
            let Some(rower) = Rower::get(conn, id)? else {
                return Ok(None);
            };
            let seat_affinities = SeatAffinity::list_for_rower(conn, id)?;
            let pair_affinities = PairAffinity::list_for_rower(conn, id)?;
            let mut other_rowers: Vec<Rower> = Rower::list_active(conn)?
                .into_iter()
                .filter(|r| r.id != id)
                .collect();
            other_rowers.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(Some(RowerDetail {
                rower,
                seat_affinities,
                pair_affinities,
                other_rowers,
            }))
        })
        .await
        .map_err(internal_error)?;
    maybe.ok_or_else(|| not_found("Rower not found."))
}

// ---- Seat affinities ------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct SeatAffinityInput {
    pub(crate) zone: String,
    pub(crate) weight: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SeatAffinityDelete {
    pub(crate) zone: String,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn seat_affinity_upsert_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<SeatAffinityInput>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let weight = match validate_weight(input.weight) {
        Ok(w) => w,
        Err(msg) => return seat_section_with_error(&tenant.db, id, &msg).await,
    };
    let zone = match SeatZone::from_str_opt(&input.zone) {
        Some(z) => z,
        None => {
            return seat_section_with_error(&tenant.db, id, &format!("invalid zone: {}", input.zone)).await
        }
    };
    tenant
        .db
        .with_conn(move |conn| SeatAffinity::upsert(conn, id, zone, weight))
        .await
        .map_err(internal_error)?;
    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "rower.seat_affinity.update",
        "rower",
        &id.to_string(),
        Some(serde_json::json!({"zone": input.zone, "weight": input.weight}).to_string()),
    );
    seat_section_response(&tenant.db, id).await
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn seat_affinity_delete_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<SeatAffinityDelete>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let zone = match SeatZone::from_str_opt(&input.zone) {
        Some(z) => z,
        None => return seat_section_with_error(&tenant.db, id, "invalid zone").await,
    };
    tenant
        .db
        .with_conn(move |conn| SeatAffinity::delete(conn, id, zone))
        .await
        .map_err(internal_error)?;
    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "rower.seat_affinity.delete",
        "rower",
        &id.to_string(),
        Some(serde_json::json!({"zone": input.zone}).to_string()),
    );
    seat_section_response(&tenant.db, id).await
}

async fn seat_section_response(db: &Db, id: RowerId) -> Result<Html<String>, ErrorResponse> {
    let detail = load_detail(db, id).await?;
    Ok(Html(templates::rowers::seat_affinities_section(&detail, None, true).into_string()))
}

async fn seat_section_with_error(db: &Db, id: RowerId, msg: &str) -> Result<Html<String>, ErrorResponse> {
    let detail = load_detail(db, id).await?;
    Ok(Html(templates::rowers::seat_affinities_section(&detail, Some(msg), true).into_string()))
}

// ---- Pair affinities ------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct PairAffinityInput {
    pub(crate) partner_id: RowerId,
    pub(crate) weight: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PairAffinityDelete {
    pub(crate) partner_id: RowerId,
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn pair_affinity_upsert_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<PairAffinityInput>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    if input.partner_id == id {
        return pair_section_with_error(&tenant.db, id, "cannot pair a rower with themselves").await;
    }
    let weight = match validate_weight(input.weight) {
        Ok(w) => w,
        Err(msg) => return pair_section_with_error(&tenant.db, id, &msg).await,
    };
    let partner = input.partner_id;
    tenant
        .db
        .with_conn(move |conn| PairAffinity::upsert(conn, id, partner, weight))
        .await
        .map_err(internal_error)?;
    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "rower.pair_affinity.update",
        "rower",
        &id.to_string(),
        Some(serde_json::json!({"partner_id": partner.to_string(), "weight": input.weight}).to_string()),
    );
    pair_section_response(&tenant.db, id).await
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn pair_affinity_delete_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<PairAffinityDelete>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let partner = input.partner_id;
    tenant
        .db
        .with_conn(move |conn| PairAffinity::delete(conn, id, partner))
        .await
        .map_err(internal_error)?;
    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        "rower.pair_affinity.delete",
        "rower",
        &id.to_string(),
        Some(serde_json::json!({"partner_id": partner.to_string()}).to_string()),
    );
    pair_section_response(&tenant.db, id).await
}

async fn pair_section_response(db: &Db, id: RowerId) -> Result<Html<String>, ErrorResponse> {
    let detail = load_detail(db, id).await?;
    Ok(Html(templates::rowers::pair_affinities_section(&detail, None, true).into_string()))
}

async fn pair_section_with_error(db: &Db, id: RowerId, msg: &str) -> Result<Html<String>, ErrorResponse> {
    let detail = load_detail(db, id).await?;
    Ok(Html(templates::rowers::pair_affinities_section(&detail, Some(msg), true).into_string()))
}

// ---- Soft-delete / reactivate ---------------------------------------

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn toggle_active_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let rower = load(&tenant.db, id).await?;
    let new_active = !rower.active.as_bool();
    let rid = rower.id;

    tenant
        .db
        .with_conn(move |conn| Rower::set_active(conn, rid, new_active))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        Some(tenant.claims.user_id().as_int()),
        if new_active { "rower.reactivate" } else { "rower.deactivate" },
        "rower",
        &id.to_string(),
        None,
    );

    Ok(Html(
        r##"<div hx-get="/team/roster" hx-target="#team-tab-content" hx-trigger="load" hx-swap="innerHTML"></div>"##.to_string(),
    ))
}

fn validate_weight(weight: i32) -> Result<AffinityWeight, String> {
    if weight == 0 {
        return Err("weight cannot be zero (use ±1 for the weakest preference)".into());
    }
    if !(AFFINITY_WEIGHT_MIN..=AFFINITY_WEIGHT_MAX).contains(&weight) {
        return Err(format!(
            "weight must be between {AFFINITY_WEIGHT_MIN} and {AFFINITY_WEIGHT_MAX} (excluding zero), got {weight}"
        ));
    }
    AffinityWeight::try_new(weight)
        .ok_or_else(|| format!("invalid weight: {weight}"))
}
