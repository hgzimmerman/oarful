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
    extract::{Path, State},
    http::StatusCode,
    response::Html,
    Form,
};
use axum_htmx::HxRequest;
use lineup_db::rower::{
    types::{RowerId, RowerWeightClass, Side, SideStrength, Skill, Strength},
    Rower,
};
use lineup_db::types::IntBool;
use serde::Deserialize;

use crate::{handlers::internal_error, state::AppState, templates};

pub(crate) async fn list_handler(
    State(state): State<AppState>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let rowers = state
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
    State(state): State<AppState>,
    Path(id): Path<RowerId>,
) -> Result<Html<String>, StatusCode> {
    let rower = load(&state, id).await?;
    Ok(Html(templates::rowers::static_row(&rower).into_string()))
}

/// Return one editable `<tr>` for the given rower. Triggered by the
/// Edit button on a static row.
pub(crate) async fn edit_handler(
    State(state): State<AppState>,
    Path(id): Path<RowerId>,
) -> Result<Html<String>, StatusCode> {
    let rower = load(&state, id).await?;
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
    State(state): State<AppState>,
    Path(id): Path<RowerId>,
    Form(input): Form<RowerEditInput>,
) -> Result<Html<String>, StatusCode> {
    let mut rower = load(&state, id).await?;

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

    let saved = state
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

async fn load(state: &AppState, id: RowerId) -> Result<Rower, StatusCode> {
    let maybe = state
        .db
        .with_conn(move |conn| Rower::get(conn, id))
        .await
        .map_err(internal_error)?;
    maybe.ok_or(StatusCode::NOT_FOUND)
}
