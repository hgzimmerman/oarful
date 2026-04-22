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
    types::{BoatId, CoxPosition, WeightClass},
    Boat, NewBoat,
};
use lineup_db::rower::types::Side;
use lineup_db::state::Db;
use lineup_db::types::IntBool;
use serde::{Deserialize, Serialize};

use lineup_db::app_user::Role;

use crate::{
    handlers::{internal_error, not_found, ErrorResponse},
    state::TenantContext,
    templates,
};

/// Build the fleet list markup (shared by `/boats` and `/admin/fleet`).
pub(crate) async fn fleet_content(tenant: &TenantContext) -> Result<maud::Markup, ErrorResponse> {
    let boats = tenant
        .db
        .with_conn(|conn| Boat::list_all(conn))
        .await
        .map_err(internal_error)?;
    let can_export = tenant
        .claims
        .role()
        .unwrap_or(Role::Member)
        .at_least(Role::ProgramDirector);
    Ok(templates::boats::list_content(&boats, can_export))
}

/// `GET /boats/export.csv` — fleet CSV download. ProgramDirector+ only.
///
/// Mirrors `boat_tracking`'s `export_boats_csv_handler` format: one row
/// per boat with serde-derived headers via the `csv` crate.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn export_csv_handler(
    Extension(tenant): Extension<TenantContext>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let rows = tenant
        .db
        .with_conn(|conn| {
            let boats = Boat::list_all(conn)?;
            let mut rows = Vec::with_capacity(boats.len());
            for b in &boats {
                let usage = Boat::usage_summary(conn, b.id)?;
                rows.push(BoatCsvRow {
                    boat_id: b.id,
                    boat_name: b.name.clone(),
                    boat_type: boat_type_label(b),
                    boat_weight_class: b.weight_class,
                    manufactured_at: b.manufactured_at,
                    acquired_at: b.acquired_at,
                    relinquished_at: b.relinquished_at,
                    total_uses: usage.total_uses as u64,
                    last_used: usage.last_used,
                });
            }
            Ok(rows)
        })
        .await
        .map_err(internal_error)?;

    Ok(CsvDownload(rows))
}

/// One row in the fleet CSV export. Field order = column order.
#[derive(Serialize)]
struct BoatCsvRow {
    boat_id: BoatId,
    boat_name: String,
    boat_type: String,
    boat_weight_class: WeightClass,
    manufactured_at: Option<NaiveDate>,
    acquired_at: Option<NaiveDate>,
    relinquished_at: Option<NaiveDate>,
    total_uses: u64,
    last_used: Option<NaiveDate>,
}

/// Generic CSV download wrapper — serialises `Vec<T>` via the `csv` crate
/// and returns it as a `text/csv` attachment.
struct CsvDownload<T>(Vec<T>);

impl<T: Serialize> IntoResponse for CsvDownload<T> {
    fn into_response(self) -> axum::response::Response {
        match csv_serialize(self.0) {
            Ok(body) => (
                [
                    (axum::http::header::CONTENT_TYPE, "text/csv"),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        "attachment; filename=\"fleet.csv\"",
                    ),
                ],
                body,
            )
                .into_response(),
            Err(err) => {
                tracing::error!(?err, "CSV serialization failed");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

fn csv_serialize<T: Serialize>(rows: Vec<T>) -> Result<String, Box<dyn std::error::Error>> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(Vec::with_capacity(2048));
    for row in rows {
        writer.serialize(row)?;
    }
    Ok(String::from_utf8(writer.into_inner()?)?)
}

/// `GET /boats/usage-matrix.csv` — boat × date usage matrix. PD+ only.
///
/// Each row is a boat, each column is a practice date with committed
/// lineups, and each cell is 1 (used) or empty (not used).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn usage_matrix_csv_handler(
    Extension(tenant): Extension<TenantContext>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let (boats, usage_pairs) = tenant
        .db
        .with_conn(|conn| {
            use diesel::prelude::*;
            use lineup_db::schema::{lineup, practice};

            let boats = Boat::list_in_service(conn)?;
            let today = chrono::Local::now().date_naive();

            let pairs: Vec<(BoatId, NaiveDate)> = lineup::table
                .inner_join(practice::table)
                .filter(practice::date.lt(today))
                .select((lineup::boat_id, practice::date))
                .distinct()
                .order(practice::date.asc())
                .get_results(conn)?;

            Ok((boats, pairs))
        })
        .await
        .map_err(internal_error)?;

    // Collect unique sorted dates.
    let mut dates: Vec<NaiveDate> = usage_pairs.iter().map(|(_, d)| *d).collect();
    dates.sort();
    dates.dedup();

    // Build a lookup set for O(1) cell checks.
    let used: std::collections::HashSet<(BoatId, NaiveDate)> = usage_pairs.into_iter().collect();

    // Write CSV with dynamic columns.
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::with_capacity(4096));

    // Header row: "Boat", date1, date2, ...
    let mut header = vec!["Boat".to_string()];
    header.extend(dates.iter().map(|d| d.to_string()));
    wtr.write_record(&header).map_err(|e| internal_error(e))?;

    // Data rows.
    for boat in &boats {
        let mut row = vec![boat.name.clone()];
        for date in &dates {
            if used.contains(&(boat.id, *date)) {
                row.push("1".to_string());
            } else {
                row.push(String::new());
            }
        }
        wtr.write_record(&row).map_err(|e| internal_error(e))?;
    }

    let body = String::from_utf8(wtr.into_inner().map_err(|e| internal_error(e))?)
        .map_err(|e| internal_error(e))?;

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"usage-matrix.csv\"",
            ),
        ],
        body,
    ))
}

/// `GET /boats/new` — empty creation form.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn new_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let content = templates::boats::form_content(FormMode::New, &BoatFormData::empty(), None);
    Ok(super::maybe_page_authed("New boat", content, hx, &tenant))
}

/// `POST /boats` — create a new boat from the form.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn create_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<BoatFormInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let parsed = match parse_input(&input) {
        Ok(p) => p,
        Err(msg) => {
            let data = BoatFormData::from_input(&input);
            let content = templates::boats::form_content(FormMode::New, &data, Some(&msg));
            return Ok(Html(content.into_string()).into_response());
        }
    };

    let boat = tenant
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
                    cox_position: parsed.cox_position,
                },
            )
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "boat.create",
        "boat",
        &boat.id.to_string(),
        Some(serde_json::json!({"name": boat.name}).to_string()),
    );

    redirect_or_list(&tenant.db, hx).await
}

/// `GET /boats/{id}` — read-only detail page with usage stats.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn detail_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<BoatId>,
    hx: HxRequest,
) -> Result<Html<String>, ErrorResponse> {
    let (boat, usage) = tenant
        .db
        .with_conn(move |conn| {
            let boat = Boat::get(conn, id)?.ok_or(diesel::result::Error::NotFound)?;
            let usage = Boat::usage_summary(conn, id)?;
            Ok((boat, usage))
        })
        .await
        .map_err(internal_error)?;
    let can_edit = tenant
        .claims
        .role()
        .unwrap_or(Role::Member)
        .at_least(Role::ProgramDirector);
    let content = templates::boats::detail_content(&boat, &usage, can_edit);
    Ok(super::maybe_page_authed(&boat.name, content, hx, &tenant))
}

/// `GET /boats/{id}/edit` — edit form pre-filled with current values.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn edit_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<BoatId>,
    hx: HxRequest,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let boat = load(&tenant.db, id).await?;
    let data = BoatFormData::from_boat(&boat);
    let content = templates::boats::form_content(FormMode::Edit(id), &data, None);
    Ok(super::maybe_page_authed(
        &format!("{} — edit", boat.name),
        content,
        hx,
        &tenant,
    ))
}

/// `PUT /boats/{id}` — update an existing boat. Also accepts POST as
/// a fallback for non-JS form submissions (HTML forms don't support
/// PUT natively).
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn update_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<BoatId>,
    hx: HxRequest,
    Form(input): Form<BoatFormInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let parsed = match parse_input(&input) {
        Ok(p) => p,
        Err(msg) => {
            let data = BoatFormData::from_input(&input);
            let content = templates::boats::form_content(FormMode::Edit(id), &data, Some(&msg));
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

    let saved = tenant
        .db
        .with_conn(move |conn| Boat::save(conn, &boat))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "boat.update",
        "boat",
        &id.to_string(),
        Some(serde_json::json!({"name": saved.name}).to_string()),
    );

    redirect_or_list(&tenant.db, hx).await
}

/// HTMX requests get 200 + the boats list content (avoiding a
/// redirect round-trip). Non-JS falls back to 303 → /boats.
async fn redirect_or_list(
    db: &Db,
    HxRequest(is_htmx): HxRequest,
) -> Result<axum::response::Response, ErrorResponse> {
    if is_htmx {
        let boats = db
            .with_conn(|conn| Boat::list_all(conn))
            .await
            .map_err(internal_error)?;
        // Only PDs can create/update boats, so can_export is always true here.
        Ok(Html(templates::boats::list_content(&boats, true).into_string()).into_response())
    } else {
        Ok(Redirect::to("/admin/fleet").into_response())
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
    pub(crate) cox_position: String,
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
    pub(crate) cox_position: String,
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
            cox_position: "Stern".into(),
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
            cox_position: input.cox_position.clone(),
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
            cox_position: boat.cox_position.to_string(),
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
        _ => format!(
            "{}x{}{}",
            b.seat_count,
            b.oars_per_seat,
            if b.has_cox.as_bool() { "+" } else { "" }
        ),
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
    cox_position: CoxPosition,
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
        other => {
            return Err(format!(
                "invalid stroke side: {other} (must be Port or Starboard)"
            ))
        }
    };

    // 8s are always stern-loaded; for smaller coxed boats, parse the input.
    let cox_position = if seat_count >= 8 {
        CoxPosition::Stern
    } else {
        match input.cox_position.as_str() {
            "Bow" => CoxPosition::Bow,
            "Stern" => CoxPosition::Stern,
            _ => CoxPosition::Bow, // default for 4+
        }
    };

    let acquired_at =
        parse_optional_date(&input.acquired_at).map_err(|e| format!("acquired date: {e}"))?;
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
        cox_position,
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

async fn load(db: &Db, id: BoatId) -> Result<Boat, ErrorResponse> {
    let maybe = db
        .with_conn(move |conn| Boat::get(conn, id))
        .await
        .map_err(internal_error)?;
    maybe.ok_or_else(|| not_found("Boat not found."))
}

/// Public re-export of the type label helper so templates can use it
/// for the list view without importing boat db types.
pub(crate) fn type_label(b: &Boat) -> String {
    boat_type_label(b)
}
