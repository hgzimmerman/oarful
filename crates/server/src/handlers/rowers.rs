//! Rower roster — list view + per-row inline edit.
//!
//! Editing flow is HTMX-driven and never leaves the page:
//!
//! 1. The static row has an "Edit" button that `hx-get`s
//!    `/rowers/{id}/edit`, swapping `outerHTML` of the closest `<tr>`.
//! 2. The edit row contains form inputs and a Save / Cancel pair.
//!    Save `hx-post`s `/rowers/{id}` with `hx-include="closest tr"`,
//!    again swapping the row.
//! 3. Cancel `hx-get`s `/rowers/{id}/row` to fetch the canonical
//!    static row again.
//!
//! Validation errors re-render the edit row with an inline message
//! so the user keeps their entered values.

use axum::{
    extract::Path,
    http::StatusCode,
    response::Html,
    Extension, Form,
};
use axum_htmx::HxRequest;
use lineup_db::pair_affinity::PairAffinity;
use lineup_db::rower::{
    types::{RowerId, RowerWeightClass, Side, SideStrength, Skill, Strength},
    Rower,
};
use lineup_db::seat_affinity::SeatAffinity;
use lineup_db::state::Db;
use lineup_db::types::{AffinityWeight, IntBool, AFFINITY_WEIGHT_MAX, AFFINITY_WEIGHT_MIN};
use serde::Deserialize;

use crate::{handlers::internal_error, state::TenantContext, templates};

pub(crate) async fn list_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let rowers = tenant
        .db
        .with_conn(|conn| Rower::list_active(conn))
        .await
        .map_err(internal_error)?;
    let content = templates::rowers::list_content(&rowers);
    Ok(super::maybe_page("Rowers", content, hx))
}

/// Return one canonical static `<tr>` for the given rower. Used by
/// the Cancel button to undo an in-progress edit.
pub(crate) async fn row_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
) -> Result<Html<String>, StatusCode> {
    let rower = load(&tenant.db, id).await?;
    Ok(Html(templates::rowers::static_row(&rower).into_string()))
}

/// Return one editable `<tr>` for the given rower. Triggered by the
/// Edit button on a static row.
pub(crate) async fn edit_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
) -> Result<Html<String>, StatusCode> {
    let rower = load(&tenant.db, id).await?;
    Ok(Html(
        templates::rowers::edit_row(&rower, None).into_string(),
    ))
}

/// Form payload from the inline edit row. Checkbox fields arrive as
/// `Some("on")` when checked and absent when unchecked — we collapse
/// to bool at validation time.
#[derive(Debug, Deserialize)]
pub(crate) struct RowerEditInput {
    pub(crate) weight_class: String,
    pub(crate) skill: String,
    pub(crate) strength: String,
    pub(crate) side: String,
    pub(crate) side_strength: i32,
    #[serde(default)]
    pub(crate) can_cox: Option<String>,
    #[serde(default)]
    pub(crate) can_scull: Option<String>,
    #[serde(default)]
    pub(crate) is_designated_cox: Option<String>,
}

pub(crate) async fn update_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<RowerEditInput>,
) -> Result<Html<String>, StatusCode> {
    let mut rower = load(&tenant.db, id).await?;

    // Parse string enums into typed values. Any unknown variant gets
    // funneled into the inline error path so the user can correct it
    // without losing the rest of the row.
    let parsed = parse_input(&input);
    let typed = match parsed {
        Ok(typed) => typed,
        Err(msg) => {
            return Ok(Html(
                templates::rowers::edit_row(&rower, Some(&msg)).into_string(),
            ));
        }
    };

    rower.weight_class = typed.weight_class;
    rower.skill = typed.skill;
    rower.strength = typed.strength;
    rower.side = typed.side;
    rower.side_strength = typed.side_strength;
    rower.can_cox = IntBool::new(typed.can_cox);
    rower.can_scull = IntBool::new(typed.can_scull);
    rower.is_designated_cox = IntBool::new(typed.is_designated_cox);

    let saved = tenant
        .db
        .with_conn(move |conn| Rower::save(conn, &rower))
        .await
        .map_err(internal_error)?;

    Ok(Html(templates::rowers::static_row(&saved).into_string()))
}

/// Typed projection of [`RowerEditInput`] after enum parsing.
struct ParsedEdit {
    weight_class: RowerWeightClass,
    skill: Skill,
    strength: Strength,
    side: Side,
    side_strength: SideStrength,
    can_cox: bool,
    can_scull: bool,
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
    // SideStrength::new clamps internally, but the explicit range
    // check above gives a friendlier message before we get there.
    let side_strength = SideStrength::new(input.side_strength);

    Ok(ParsedEdit {
        weight_class,
        skill,
        strength,
        side,
        side_strength,
        can_cox: input.can_cox.is_some(),
        can_scull: input.can_scull.is_some(),
        is_designated_cox: input.is_designated_cox.is_some(),
    })
}

async fn load(db: &Db, id: RowerId) -> Result<Rower, StatusCode> {
    let maybe = db
        .with_conn(move |conn| Rower::get(conn, id))
        .await
        .map_err(internal_error)?;
    maybe.ok_or(StatusCode::NOT_FOUND)
}

// =====================================================================
// Per-rower detail page + affinity CRUD
// =====================================================================
//
// `GET /rowers/{id}` renders a detail page composed of three sections:
//   1. attribute summary (read-only — editing lives on the list view)
//   2. seat affinities (S3) — table + add form
//   3. pair affinities (S2) — table + add form
//
// Each affinity mutation hits a small POST endpoint that returns just
// the affected `<section>` so HTMX can `outerHTML`-swap it without
// re-rendering the whole page. The section partial functions in the
// template module are exposed so handlers can call them directly.

/// `GET /rowers/{id}` — full detail page.
pub(crate) async fn detail_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let detail = load_detail(&tenant.db, id).await?;
    let content = templates::rowers::detail_content(&detail);
    Ok(super::maybe_page(
        &format!("Rower · {}", detail.rower.name),
        content,
        hx,
    ))
}

/// Bundled lookup for the detail page: rower + their affinities + the
/// roster of every other active rower (used by the partner picker on
/// the pair-affinity add form).
pub(crate) struct RowerDetail {
    pub(crate) rower: Rower,
    pub(crate) seat_affinities: Vec<SeatAffinity>,
    pub(crate) pair_affinities: Vec<PairAffinity>,
    pub(crate) other_rowers: Vec<Rower>,
}

async fn load_detail(db: &Db, id: RowerId) -> Result<RowerDetail, StatusCode> {
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
    maybe.ok_or(StatusCode::NOT_FOUND)
}

// ---- Seat affinities ------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct SeatAffinityInput {
    pub(crate) seat_position: i32,
    pub(crate) weight: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SeatAffinityDelete {
    pub(crate) seat_position: i32,
}

/// `POST /rowers/{id}/seat-affinity` — upsert one (rower, seat) row.
pub(crate) async fn seat_affinity_upsert_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<SeatAffinityInput>,
) -> Result<Html<String>, StatusCode> {
    let weight = match validate_weight(input.weight) {
        Ok(w) => w,
        Err(msg) => return seat_section_with_error(&tenant.db, id, &msg).await,
    };
    if !(1..=8).contains(&input.seat_position) {
        return seat_section_with_error(
            &tenant.db,
            id,
            &format!(
                "seat position must be between 1 and 8, got {}",
                input.seat_position
            ),
        )
        .await;
    }
    let seat = input.seat_position;
    tenant
        .db
        .with_conn(move |conn| SeatAffinity::upsert(conn, id, seat, weight))
        .await
        .map_err(internal_error)?;
    seat_section_response(&tenant.db, id).await
}

/// `POST /rowers/{id}/seat-affinity/delete` — drop one (rower, seat).
pub(crate) async fn seat_affinity_delete_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<SeatAffinityDelete>,
) -> Result<Html<String>, StatusCode> {
    let seat = input.seat_position;
    tenant
        .db
        .with_conn(move |conn| SeatAffinity::delete(conn, id, seat))
        .await
        .map_err(internal_error)?;
    seat_section_response(&tenant.db, id).await
}

async fn seat_section_response(
    db: &Db,
    id: RowerId,
) -> Result<Html<String>, StatusCode> {
    let detail = load_detail(db, id).await?;
    Ok(Html(
        templates::rowers::seat_affinities_section(&detail, None).into_string(),
    ))
}

async fn seat_section_with_error(
    db: &Db,
    id: RowerId,
    msg: &str,
) -> Result<Html<String>, StatusCode> {
    let detail = load_detail(db, id).await?;
    Ok(Html(
        templates::rowers::seat_affinities_section(&detail, Some(msg)).into_string(),
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

/// `POST /rowers/{id}/pair-affinity` — upsert one canonical pair.
pub(crate) async fn pair_affinity_upsert_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<PairAffinityInput>,
) -> Result<Html<String>, StatusCode> {
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
    pair_section_response(&tenant.db, id).await
}

/// `POST /rowers/{id}/pair-affinity/delete` — drop one canonical pair.
pub(crate) async fn pair_affinity_delete_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<RowerId>,
    Form(input): Form<PairAffinityDelete>,
) -> Result<Html<String>, StatusCode> {
    let partner = input.partner_id;
    tenant
        .db
        .with_conn(move |conn| PairAffinity::delete(conn, id, partner))
        .await
        .map_err(internal_error)?;
    pair_section_response(&tenant.db, id).await
}

async fn pair_section_response(
    db: &Db,
    id: RowerId,
) -> Result<Html<String>, StatusCode> {
    let detail = load_detail(db, id).await?;
    Ok(Html(
        templates::rowers::pair_affinities_section(&detail, None).into_string(),
    ))
}

async fn pair_section_with_error(
    db: &Db,
    id: RowerId,
    msg: &str,
) -> Result<Html<String>, StatusCode> {
    let detail = load_detail(db, id).await?;
    Ok(Html(
        templates::rowers::pair_affinities_section(&detail, Some(msg)).into_string(),
    ))
}

/// Validate a coach-supplied affinity weight against the documented
/// `[-5, 5] \ {0}` range. Returns the typed [`AffinityWeight`] on
/// success or a friendly error message on rejection.
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
