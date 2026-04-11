//! Boat CRUD — list, create, edit.
//!
//! Mirrors `boat_tracking/src/handlers/boats.rs` in spirit but uses
//! this project's `db.with_conn` pattern and separate form pages (not
//! inline editing) since boats have too many fields for a single
//! table row.

use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    Extension, Form,
};
use axum_htmx::HxRequest;
use chrono::NaiveDate;
use lineup_db::boat::{
    types::{BoatId, WeightClass},
    Boat, NewBoat,
};
use lineup_db::rower::types::Side;
use lineup_db::state::Db;
use lineup_db::types::IntBool;
use serde::Deserialize;

use lineup_db::app_user::Role;

use crate::{handlers::internal_error, state::TenantContext, templates};

/// `GET /boats` — full fleet list.
pub(crate) async fn list_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    let boats = tenant
        .db
        .with_conn(|conn| Boat::list_all(conn))
        .await
        .map_err(internal_error)?;
    let content = templates::boats::list_content(&boats);
    Ok(super::maybe_page("Boats", content, hx))
}

/// `GET /boats/new` — empty creation form.
pub(crate) async fn new_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let content = templates::boats::form_content(FormMode::New, &BoatFormData::empty(), None);
    Ok(super::maybe_page("New boat", content, hx))
}

/// `POST /boats` — create a new boat from the form.
pub(crate) async fn create_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<BoatFormInput>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let parsed = match parse_input(&input) {
        Ok(p) => p,
        Err(msg) => {
            let data = BoatFormData::from_input(&input);
            let content =
                templates::boats::form_content(FormMode::New, &data, Some(&msg));
            return Ok(Html(content.into_string()).into_response());
        }
    };

    tenant
        .db
        .with_conn(move |conn| {
            Boat::insert(
                conn,
                NewBoat {
                    name: parsed.name,
                    weight_class: parsed.weight_class,
                    seat_count: parsed.seat_count,
                    has_cox: IntBool::new(parsed.has_cox),
                    oars_per_seat: parsed.oars_per_seat,
                    acquired_at: parsed.acquired_at,
                    manufactured_at: parsed.manufactured_at,
                    stroke_side: parsed.stroke_side,
                },
            )
        })
        .await
        .map_err(internal_error)?;

    redirect_or_list(&tenant.db, hx).await
}

/// `GET /boats/{id}` — edit form pre-filled with current values.
pub(crate) async fn edit_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<BoatId>,
    hx: HxRequest,
) -> Result<Html<String>, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let boat = load(&tenant.db, id).await?;
    let data = BoatFormData::from_boat(&boat);
    let content =
        templates::boats::form_content(FormMode::Edit(id), &data, None);
    Ok(super::maybe_page(
        &format!("{} — edit", boat.name),
        content,
        hx,
    ))
}

/// `PUT /boats/{id}` — update an existing boat. Also accepts POST as
/// a fallback for non-JS form submissions (HTML forms don't support
/// PUT natively).
pub(crate) async fn update_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<BoatId>,
    hx: HxRequest,
    Form(input): Form<BoatFormInput>,
) -> Result<impl IntoResponse, StatusCode> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let parsed = match parse_input(&input) {
        Ok(p) => p,
        Err(msg) => {
            let data = BoatFormData::from_input(&input);
            let content =
                templates::boats::form_content(FormMode::Edit(id), &data, Some(&msg));
            return Ok(Html(content.into_string()).into_response());
        }
    };

    let mut boat = load(&tenant.db, id).await?;
    boat.name = parsed.name;
    boat.weight_class = parsed.weight_class;
    boat.seat_count = parsed.seat_count;
    boat.has_cox = IntBool::new(parsed.has_cox);
    boat.oars_per_seat = parsed.oars_per_seat;
    boat.acquired_at = parsed.acquired_at;
    boat.manufactured_at = parsed.manufactured_at;
    boat.relinquished_at = parsed.relinquished_at;
    boat.stroke_side = parsed.stroke_side;

    tenant
        .db
        .with_conn(move |conn| Boat::save(conn, &boat))
        .await
        .map_err(internal_error)?;

    redirect_or_list(&tenant.db, hx).await
}

/// HTMX requests get 200 + the boats list content (avoiding a
/// redirect round-trip). Non-JS falls back to 303 → /boats.
async fn redirect_or_list(
    db: &Db,
    HxRequest(is_htmx): HxRequest,
) -> Result<axum::response::Response, StatusCode> {
    if is_htmx {
        let boats = db
            .with_conn(|conn| Boat::list_all(conn))
            .await
            .map_err(internal_error)?;
        Ok(Html(templates::boats::list_content(&boats).into_string()).into_response())
    } else {
        Ok(Redirect::to("/boats").into_response())
    }
}

// =====================================================================
// Form helpers
// =====================================================================

/// Discriminator for the shared add/edit form template.
#[derive(Debug, Clone, Copy)]
pub(crate) enum FormMode {
    New,
    Edit(BoatId),
}

/// Raw form payload. All strings — parsing happens in `parse_input`.
#[derive(Debug, Deserialize)]
pub(crate) struct BoatFormInput {
    pub(crate) name: String,
    pub(crate) weight_class: String,
    pub(crate) boat_type: String,
    pub(crate) stroke_side: String,
    #[serde(default)]
    pub(crate) acquired_at: String,
    #[serde(default)]
    pub(crate) manufactured_at: String,
    #[serde(default)]
    pub(crate) relinquished_at: String,
}

/// Typed values for rendering the form (pre-filling selects, etc.).
/// Kept as strings so the template doesn't need to import db types.
pub(crate) struct BoatFormData {
    pub(crate) name: String,
    pub(crate) weight_class: String,
    pub(crate) boat_type: String,
    pub(crate) stroke_side: String,
    pub(crate) acquired_at: String,
    pub(crate) manufactured_at: String,
    pub(crate) relinquished_at: String,
}

impl BoatFormData {
    pub(crate) fn empty() -> Self {
        Self {
            name: String::new(),
            weight_class: "Medium".into(),
            boat_type: "Eight".into(),
            stroke_side: "Port".into(),
            acquired_at: String::new(),
            manufactured_at: String::new(),
            relinquished_at: String::new(),
        }
    }

    fn from_input(input: &BoatFormInput) -> Self {
        Self {
            name: input.name.clone(),
            weight_class: input.weight_class.clone(),
            boat_type: input.boat_type.clone(),
            stroke_side: input.stroke_side.clone(),
            acquired_at: input.acquired_at.clone(),
            manufactured_at: input.manufactured_at.clone(),
            relinquished_at: input.relinquished_at.clone(),
        }
    }

    fn from_boat(boat: &Boat) -> Self {
        Self {
            name: boat.name.clone(),
            weight_class: boat.weight_class.to_string(),
            boat_type: boat_type_label(boat),
            stroke_side: match boat.stroke_side {
                Side::Port => "Port",
                Side::Starboard => "Starboard",
                Side::Either => "Starboard", // shouldn't happen, fallback
            }
            .into(),
            acquired_at: boat
                .acquired_at
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            manufactured_at: boat
                .manufactured_at
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            relinquished_at: boat
                .relinquished_at
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
        }
    }
}

/// Derive a display label from a boat's (seat_count, has_cox, oars_per_seat).
fn boat_type_label(b: &Boat) -> String {
    match (b.seat_count, b.has_cox.as_bool(), b.oars_per_seat) {
        (1, false, 2) => "Single".into(),
        (2, false, 2) => "Double".into(),
        (2, false, 1) => "Pair".into(),
        (4, false, 2) => "Quad".into(),
        (4, true, 2) => "QuadPlus".into(),
        (4, false, 1) => "Four".into(),
        (4, true, 1) => "FourPlus".into(),
        (8, true, 1) => "Eight".into(),
        (8, false, 1) => "CoxlessEight".into(),
        _ => format!("{}x{}{}", b.seat_count, b.oars_per_seat, if b.has_cox.as_bool() { "+" } else { "" }),
    }
}

/// Typed projection after input validation.
struct ParsedBoat {
    name: String,
    weight_class: WeightClass,
    seat_count: i32,
    has_cox: bool,
    oars_per_seat: i32,
    stroke_side: Side,
    acquired_at: Option<NaiveDate>,
    manufactured_at: Option<NaiveDate>,
    relinquished_at: Option<NaiveDate>,
}

fn parse_input(input: &BoatFormInput) -> Result<ParsedBoat, String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("boat name is required".into());
    }

    let weight_class = match input.weight_class.as_str() {
        "Light" => WeightClass::Light,
        "Medium" => WeightClass::Medium,
        "Heavy" => WeightClass::Heavy,
        "Tubby" => WeightClass::Tubby,
        other => return Err(format!("invalid weight class: {other}")),
    };

    let (seat_count, has_cox, oars_per_seat) = match input.boat_type.as_str() {
        "Single" => (1, false, 2),
        "Double" => (2, false, 2),
        "Pair" => (2, false, 1),
        "Quad" => (4, false, 2),
        "QuadPlus" => (4, true, 2),
        "Four" => (4, false, 1),
        "FourPlus" => (4, true, 1),
        "Eight" => (8, true, 1),
        "CoxlessEight" => (8, false, 1),
        other => return Err(format!("invalid boat type: {other}")),
    };

    let stroke_side = match input.stroke_side.as_str() {
        "Port" => Side::Port,
        "Starboard" => Side::Starboard,
        other => return Err(format!("invalid stroke side: {other} (must be Port or Starboard)")),
    };

    let acquired_at = parse_optional_date(&input.acquired_at)
        .map_err(|e| format!("acquired date: {e}"))?;
    let manufactured_at = parse_optional_date(&input.manufactured_at)
        .map_err(|e| format!("manufactured date: {e}"))?;
    let relinquished_at = parse_optional_date(&input.relinquished_at)
        .map_err(|e| format!("relinquished date: {e}"))?;

    Ok(ParsedBoat {
        name,
        weight_class,
        seat_count,
        has_cox,
        oars_per_seat,
        stroke_side,
        acquired_at,
        manufactured_at,
        relinquished_at,
    })
}

fn parse_optional_date(s: &str) -> Result<Option<NaiveDate>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(Some)
        .map_err(|e| format!("invalid date '{s}': {e}"))
}

async fn load(db: &Db, id: BoatId) -> Result<Boat, StatusCode> {
    let maybe = db
        .with_conn(move |conn| Boat::get(conn, id))
        .await
        .map_err(internal_error)?;
    maybe.ok_or(StatusCode::NOT_FOUND)
}

/// Public re-export of the type label helper so templates can use it
/// for the list view without importing boat db types.
pub(crate) fn type_label(b: &Boat) -> String {
    boat_type_label(b)
}
