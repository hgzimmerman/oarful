//! Per-rower detail page: attributes, seat affinities, pair affinities,
//! soft-delete/reactivate.

use axum::{extract::Path, http::StatusCode, response::Html, Extension, Form};
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
use lineup_db::types::{HeightM, WeightKg};
use maud::html;
use serde::Deserialize;

use lineup_db::app_user::{AppUser, Role};
use lineup_db::team::{BucketVisibility, Team};

use crate::{
    handlers::{internal_error, not_found, ErrorResponse},
    state::TenantContext,
    templates,
};

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
    let locked = locked_bucket_fields_for_rower(&tenant, &jar, &rower).await?;
    Ok(Html(
        templates::rowers::attribute_edit_section(&rower, None, &perms, &locked).into_string(),
    ))
}

#[derive(Debug, Deserialize)]
pub(crate) struct RowerEditInput {
    #[serde(default)]
    pub(crate) first_name: Option<String>,
    #[serde(default)]
    pub(crate) last_name: Option<String>,
    pub(crate) weight_class: RowerWeightClass,
    pub(crate) skill: Skill,
    pub(crate) strength: Strength,
    pub(crate) height: Height,
    pub(crate) side: Side,
    pub(crate) side_strength: i32,
    #[serde(default)]
    pub(crate) can_cox: Option<String>,
    #[serde(default)]
    pub(crate) sweep_bias: i32,
    #[serde(default)]
    pub(crate) is_designated_cox: Option<String>,
    #[serde(default)]
    pub(crate) weight_lbs: Option<String>,
    #[serde(default)]
    pub(crate) height_in: Option<String>,
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

    // Name — always editable by both members and coaches.
    rower.first_name = input
        .first_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    rower.last_name = input
        .last_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if perms.can_edit("weight_class") {
        rower.weight_class = input.weight_class;
    }
    if perms.can_edit("skill") {
        rower.skill = input.skill;
    }
    if perms.can_edit("strength") {
        rower.strength = input.strength;
    }
    if perms.can_edit("height") {
        rower.height = input.height;
    }
    if perms.can_edit("side") {
        rower.side = input.side;
    }
    if perms.can_edit("side_strength") {
        rower.side_strength = SideStrength::new(input.side_strength);
    }
    if perms.can_edit("can_cox") {
        rower.can_cox = IntBool::new(input.can_cox.is_some());
    }
    if perms.can_edit("sweep_bias") {
        rower.sweep_bias = SweepBias::new(input.sweep_bias);
    }
    if perms.can_edit("is_designated_cox") {
        rower.is_designated_cox = IntBool::new(input.is_designated_cox.is_some());
    }

    // Raw metrics — parse lbs→kg and inches→metres.
    rower.weight_kg = input
        .weight_lbs
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<f64>().ok())
        .map(|lbs| WeightKg::new(lbs / 2.20462));
    rower.height_m = input
        .height_in
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<f64>().ok())
        .map(|inches| HeightM::new(inches * 0.0254));

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
        None,
    );
    tenant.complete_onboarding_step(lineup_db::onboarding::OnboardingStep::CustomizeRower);

    let mut body = templates::rowers::attribute_section(&saved, None, &perms).into_string();
    // OOB swap to keep the page header name in sync.
    body.push_str(
        &html! {
            h1 #rower-name "hx-swap-oob"="true"
               class="text-2xl font-bold text-ink" {
                (saved.display_name())
            }
        }
        .into_string(),
    );
    Ok(Html(body))
}

/// Compute which bucket fields are locked (auto-derived from raw values + thresholds).
pub(crate) async fn locked_bucket_fields_for_rower(
    tenant: &TenantContext,
    jar: &CookieJar,
    rower: &Rower,
) -> Result<templates::rowers::LockedBuckets, ErrorResponse> {
    let team_id = crate::handlers::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let thresholds = tenant
        .db
        .with_conn(move |conn| lineup_db::team_threshold::TeamThreshold::for_team(conn, team_id))
        .await
        .map_err(internal_error)?;

    let has_weight_t = thresholds
        .iter()
        .any(|t| t.metric == lineup_db::team_threshold::METRIC_WEIGHT);
    let has_height_t = thresholds
        .iter()
        .any(|t| t.metric == lineup_db::team_threshold::METRIC_HEIGHT);
    let has_strength_t = thresholds
        .iter()
        .any(|t| t.metric == lineup_db::team_threshold::METRIC_STRENGTH);

    // Strength is locked if the rower has an erg test at the team's threshold distance.
    let strength_locked = if has_strength_t {
        let rid = rower.id;
        let team = tenant
            .db
            .with_conn(move |conn| lineup_db::team::Team::get(conn, team_id))
            .await
            .map_err(internal_error)?;
        if let Some(dist) = team.and_then(|t| t.erg_threshold_distance_m) {
            let tests = tenant
                .db
                .with_conn(move |conn| lineup_db::erg_test::ErgTest::list_for_rower(conn, rid))
                .await
                .map_err(internal_error)?;
            tests.iter().any(|t| t.distance_m == dist)
        } else {
            false
        }
    } else {
        false
    };

    Ok(templates::rowers::LockedBuckets {
        weight: rower.weight_kg.is_some() && has_weight_t,
        height: rower.height_m.is_some() && has_height_t,
        strength: strength_locked,
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
    let is_coach = tenant.claims.role().at_least(Role::Coach);
    if is_coach {
        return Ok(templates::rowers::DetailPermissions::coach());
    }
    let uid = tenant
        .claims
        .user_id()
        .ok_or_else(|| super::super::bad_request("Not available in superuser view."))?;
    let owns_rower = tenant
        .db
        .with_conn(move |conn| {
            let user = AppUser::get(conn, uid)?;
            Ok(user.and_then(|u| u.rower_id) == Some(rower_id))
        })
        .await
        .map_err(internal_error)?;
    if !owns_rower {
        return Err(crate::handlers::ErrorResponse(
            StatusCode::FORBIDDEN,
            "You don't have permission to perform this action.".into(),
        ));
    }
    let team_id = crate::handlers::active_team(&tenant.db, jar, Some(&tenant.claims)).await?;
    let team = tenant
        .db
        .with_conn(move |conn| Team::get(conn, team_id))
        .await
        .map_err(internal_error)?;
    let bv = team
        .as_ref()
        .map(|t| t.bucket_visibility)
        .unwrap_or(BucketVisibility::Off);
    let mrm = team
        .as_ref()
        .map(|t| t.member_raw_metrics.as_bool())
        .unwrap_or(false);
    Ok(templates::rowers::DetailPermissions::member(bv, mrm))
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
    let show_emails = tenant.show_emails();
    let content = templates::rowers::detail_content(&detail, perms, show_emails);
    Ok(crate::handlers::maybe_page_authed(
        &format!("Rower · {}", detail.rower.display_name()),
        content,
        hx,
        &tenant,
    ))
}

pub(crate) struct RowerDetail {
    pub(crate) rower: Rower,
    pub(crate) email: Option<String>,
    pub(crate) seat_affinities: Vec<SeatAffinity>,
    pub(crate) pair_affinities: Vec<PairAffinity>,
    pub(crate) other_rowers: Vec<Rower>,
    pub(crate) erg_tests: Vec<lineup_db::erg_test::ErgTest>,
}

pub(crate) async fn load_detail(db: &Db, id: RowerId) -> Result<RowerDetail, ErrorResponse> {
    let maybe = db
        .with_conn(move |conn| {
            let Some(rower) = Rower::get(conn, id)? else {
                return Ok(None);
            };
            let email = AppUser::find_by_rower_id(conn, rower.id)?.map(|u| u.email);
            let seat_affinities = SeatAffinity::list_for_rower(conn, id)?;
            let pair_affinities = PairAffinity::list_for_rower(conn, id)?;
            let mut other_rowers: Vec<Rower> = Rower::list_active(conn)?
                .into_iter()
                .filter(|r| r.id != id)
                .collect();
            other_rowers.sort_by_key(|a| a.display_name());
            let erg_tests = lineup_db::erg_test::ErgTest::list_for_rower(conn, id)?;
            Ok(Some(RowerDetail {
                rower,
                email,
                seat_affinities,
                pair_affinities,
                other_rowers,
                erg_tests,
            }))
        })
        .await
        .map_err(internal_error)?;
    maybe.ok_or_else(|| not_found("Rower not found."))
}

// ---- Seat affinities ------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct SeatAffinityInput {
    pub(crate) zone: SeatZone,
    pub(crate) weight: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SeatAffinityDelete {
    pub(crate) zone: SeatZone,
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
    let zone = input.zone;
    tenant
        .db
        .with_conn(move |conn| SeatAffinity::upsert(conn, id, zone, weight))
        .await
        .map_err(internal_error)?;
    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "rower.seat_affinity.update",
        "rower",
        &id.to_string(),
        Some(serde_json::json!({"zone": input.zone, "weight": input.weight}).to_string()),
    );
    tenant.complete_onboarding_step(lineup_db::onboarding::OnboardingStep::CustomizeRower);
    seat_section_response(&tenant.db, id).await
}

#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn seat_affinity_delete_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<SeatAffinityDelete>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;
    let zone = input.zone;
    tenant
        .db
        .with_conn(move |conn| SeatAffinity::delete(conn, id, zone))
        .await
        .map_err(internal_error)?;
    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "rower.seat_affinity.delete",
        "rower",
        &id.to_string(),
        Some(serde_json::json!({"zone": zone}).to_string()),
    );
    seat_section_response(&tenant.db, id).await
}

async fn seat_section_response(db: &Db, id: RowerId) -> Result<Html<String>, ErrorResponse> {
    let detail = load_detail(db, id).await?;
    Ok(Html(
        templates::rowers::seat_affinities_section(&detail, None, true).into_string(),
    ))
}

async fn seat_section_with_error(
    db: &Db,
    id: RowerId,
    msg: &str,
) -> Result<Html<String>, ErrorResponse> {
    let detail = load_detail(db, id).await?;
    Ok(Html(
        templates::rowers::seat_affinities_section(&detail, Some(msg), true).into_string(),
    ))
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
        return pair_section_with_error(&tenant.db, id, "cannot pair a rower with themselves")
            .await;
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
        tenant.claims.audit_user_id(),
        "rower.pair_affinity.update",
        "rower",
        &id.to_string(),
        Some(
            serde_json::json!({"partner_id": partner.to_string(), "weight": input.weight})
                .to_string(),
        ),
    );
    tenant.complete_onboarding_step(lineup_db::onboarding::OnboardingStep::CustomizeRower);
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
        tenant.claims.audit_user_id(),
        "rower.pair_affinity.delete",
        "rower",
        &id.to_string(),
        Some(serde_json::json!({"partner_id": partner.to_string()}).to_string()),
    );
    pair_section_response(&tenant.db, id).await
}

async fn pair_section_response(db: &Db, id: RowerId) -> Result<Html<String>, ErrorResponse> {
    let detail = load_detail(db, id).await?;
    Ok(Html(
        templates::rowers::pair_affinities_section(&detail, None, true).into_string(),
    ))
}

async fn pair_section_with_error(
    db: &Db,
    id: RowerId,
    msg: &str,
) -> Result<Html<String>, ErrorResponse> {
    let detail = load_detail(db, id).await?;
    Ok(Html(
        templates::rowers::pair_affinities_section(&detail, Some(msg), true).into_string(),
    ))
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
        tenant.claims.audit_user_id(),
        if new_active {
            "rower.reactivate"
        } else {
            "rower.deactivate"
        },
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
    AffinityWeight::try_new(weight).ok_or_else(|| format!("invalid weight: {weight}"))
}

// ---- Erg test CRUD ---------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct ErgTestInput {
    distance_m: i32,
    time: String,
    #[serde(default)]
    rowed_at: Option<String>,
}

/// Parse "M:SS.dd" → centiseconds.
fn parse_erg_time(s: &str) -> Option<i32> {
    let (mins_str, rest) = s.split_once(':')?;
    let (secs_str, frac_str) = rest.split_once('.')?;
    let mins: i32 = mins_str.parse().ok()?;
    let secs: i32 = secs_str.parse().ok()?;
    let frac: i32 = if frac_str.len() == 1 {
        frac_str.parse::<i32>().ok()? * 10
    } else {
        frac_str.parse().ok()?
    };
    Some(mins * 6000 + secs * 100 + frac)
}

/// `POST /rowers/{id}/erg-test` — add a new erg test entry.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn erg_test_add_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<ErgTestInput>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;

    let time_cs = parse_erg_time(&input.time)
        .ok_or_else(|| crate::handlers::bad_request("Invalid time format. Use M:SS.dd"))?;

    let rowed_at = input
        .rowed_at
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    let now = chrono::Utc::now().naive_utc();
    tenant
        .db
        .with_conn(move |conn| {
            lineup_db::erg_test::ErgTest::create(
                conn,
                lineup_db::erg_test::NewErgTest {
                    rower_id: id,
                    distance_m: input.distance_m,
                    time_cs,
                    rowed_at,
                    created_at: now,
                },
            )
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "erg_test.add",
        "rower",
        &id.to_string(),
        Some(
            serde_json::json!({
                "distance_m": input.distance_m,
                "time_cs": time_cs,
            })
            .to_string(),
        ),
    );

    erg_section_response(&tenant.db, id).await
}

/// `DELETE /rowers/{id}/erg-test/{test_id}` — delete an erg test entry.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn erg_test_delete_handler(
    Extension(tenant): Extension<TenantContext>,
    Path((id, test_id)): Path<(RowerId, lineup_db::erg_test::ErgTestId)>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;

    tenant
        .db
        .with_conn(move |conn| lineup_db::erg_test::ErgTest::delete(conn, test_id))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "erg_test.delete",
        "rower",
        &id.to_string(),
        Some(serde_json::json!({"test_id": test_id}).to_string()),
    );

    erg_section_response(&tenant.db, id).await
}

async fn erg_section_response(db: &Db, id: RowerId) -> Result<Html<String>, ErrorResponse> {
    let rower = load(db, id).await?;
    let tests = db
        .with_conn(move |conn| lineup_db::erg_test::ErgTest::list_for_rower(conn, id))
        .await
        .map_err(internal_error)?;
    // Erg add/delete handlers are Coach+-gated, so always render with coach perms.
    let perms = templates::rowers::DetailPermissions::coach();
    Ok(Html(
        templates::rowers::erg_test_section_markup(&rower, &tests, &perms).into_string(),
    ))
}
