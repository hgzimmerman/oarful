//! Oar set CRUD — list, create, edit, toggle active, preference management.

use axum::{
    extract::Path,
    response::{Html, IntoResponse, Redirect},
    Extension, Form,
};
use axum_htmx::HxRequest;
use lineup_db::app_user::Role;
use lineup_db::boat::Boat;
use lineup_db::oar_set::types::OarSetId;
use lineup_db::oar_set::{NewOarSet, OarSet, OarSetPreference};
use lineup_db::state::Db;
use serde::Deserialize;

use crate::{
    handlers::{internal_error, not_found, ErrorResponse},
    state::TenantContext,
    templates,
};

/// Build the oar sets list markup.
pub(crate) async fn list_content(tenant: &TenantContext) -> Result<maud::Markup, ErrorResponse> {
    let oar_sets = tenant
        .db
        .with_conn(OarSet::list_all)
        .await
        .map_err(internal_error)?;
    Ok(templates::oar_sets::list_content(&oar_sets))
}

/// `GET /oars/new` — empty creation form.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn new_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let content = templates::oar_sets::form_content(FormMode::New, &OarSetFormData::empty(), None);
    Ok(super::maybe_page_authed(
        "New oar set",
        content,
        hx,
        &tenant,
    ))
}

/// `POST /oars` — create a new oar set.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn create_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    Form(input): Form<OarSetFormInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let parsed = match parse_input(&input) {
        Ok(p) => p,
        Err(msg) => {
            let data = OarSetFormData::from_input(&input);
            let content = templates::oar_sets::form_content(FormMode::New, &data, Some(&msg));
            return Ok(Html(content.into_string()).into_response());
        }
    };

    let oar_set = tenant
        .db
        .with_conn(move |conn| {
            OarSet::insert(
                conn,
                NewOarSet {
                    name: parsed.name,
                    oar_count: parsed.oar_count,
                    notes: parsed.notes,
                },
            )
        })
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "oar_set.create",
        "oar_set",
        &oar_set.id.to_string(),
        Some(serde_json::json!({"name": oar_set.name}).to_string()),
    );

    redirect_to_oars(&tenant, hx).await
}

/// `GET /oars/{id}` — detail page.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn detail_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<OarSetId>,
    hx: HxRequest,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let (oar_set, prefs, boats) = tenant
        .db
        .with_conn(move |conn| {
            let os = OarSet::get(conn, id)?.ok_or(diesel::result::Error::NotFound)?;
            let prefs = OarSetPreference::list_for_oar_set(conn, id)?;
            let boats = Boat::list_in_service(conn)?;
            Ok((os, prefs, boats))
        })
        .await
        .map_err(internal_error)?;

    let content = templates::oar_sets::detail_content(&oar_set, &prefs, &boats);
    Ok(super::maybe_page_authed(
        &oar_set.name,
        content,
        hx,
        &tenant,
    ))
}

/// `GET /oars/{id}/edit` — edit form.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn edit_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<OarSetId>,
    hx: HxRequest,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let oar_set = load(&tenant.db, id).await?;
    let data = OarSetFormData::from_oar_set(&oar_set);
    let content = templates::oar_sets::form_content(FormMode::Edit(id), &data, None);
    Ok(super::maybe_page_authed(
        &format!("{} — edit", oar_set.name),
        content,
        hx,
        &tenant,
    ))
}

/// `POST /oars/{id}` — update an existing oar set.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn update_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<OarSetId>,
    hx: HxRequest,
    Form(input): Form<OarSetFormInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let parsed = match parse_input(&input) {
        Ok(p) => p,
        Err(msg) => {
            let data = OarSetFormData::from_input(&input);
            let content = templates::oar_sets::form_content(FormMode::Edit(id), &data, Some(&msg));
            return Ok(Html(content.into_string()).into_response());
        }
    };

    let mut oar_set = load(&tenant.db, id).await?;
    oar_set.name = parsed.name;
    oar_set.oar_count = parsed.oar_count;
    oar_set.notes = parsed.notes;

    let saved = tenant
        .db
        .with_conn(move |conn| OarSet::save(conn, &oar_set))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        "oar_set.update",
        "oar_set",
        &id.to_string(),
        Some(serde_json::json!({"name": saved.name}).to_string()),
    );

    redirect_to_oars(&tenant, hx).await
}

/// `POST /oars/{id}/toggle-active` — toggle active/inactive.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn toggle_active_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<OarSetId>,
    hx: HxRequest,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;
    let mut oar_set = load(&tenant.db, id).await?;
    oar_set.active = lineup_db::types::IntBool::new(!oar_set.active.as_bool());

    let saved = tenant
        .db
        .with_conn(move |conn| OarSet::save(conn, &oar_set))
        .await
        .map_err(internal_error)?;

    crate::audit::record(
        &tenant.db,
        tenant.claims.audit_user_id(),
        if saved.active.as_bool() {
            "oar_set.activate"
        } else {
            "oar_set.deactivate"
        },
        "oar_set",
        &id.to_string(),
        None,
    );

    redirect_to_oars(&tenant, hx).await
}

/// `POST /oars/{id}/preferences` — save boat preferences.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn preferences_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(id): Path<OarSetId>,
    hx: HxRequest,
    Form(input): Form<PreferencesInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let prefs: Vec<(lineup_db::boat::types::BoatId, i32)> = input
        .boat_ids
        .iter()
        .enumerate()
        .filter_map(|(i, id_str)| id_str.parse().ok().map(|bid| (bid, i as i32)))
        .collect();

    tenant
        .db
        .with_conn(move |conn| OarSetPreference::replace_for_oar_set(conn, id, &prefs))
        .await
        .map_err(internal_error)?;

    // Re-render detail
    let (oar_set, new_prefs, boats) = tenant
        .db
        .with_conn(move |conn| {
            let os = OarSet::get(conn, id)?.ok_or(diesel::result::Error::NotFound)?;
            let prefs = OarSetPreference::list_for_oar_set(conn, id)?;
            let boats = Boat::list_in_service(conn)?;
            Ok((os, prefs, boats))
        })
        .await
        .map_err(internal_error)?;

    let content = templates::oar_sets::detail_content(&oar_set, &new_prefs, &boats);
    Ok(super::maybe_page_authed(
        &oar_set.name,
        content,
        hx,
        &tenant,
    ))
}

async fn redirect_to_oars(
    tenant: &TenantContext,
    HxRequest(is_htmx): HxRequest,
) -> Result<axum::response::Response, ErrorResponse> {
    if is_htmx {
        let content = list_content(tenant).await?;
        Ok(Html(content.into_string()).into_response())
    } else {
        Ok(Redirect::to("/admin/fleet/oars").into_response())
    }
}

// =====================================================================
// Form helpers
// =====================================================================

#[derive(Debug, Clone, Copy)]
pub(crate) enum FormMode {
    New,
    Edit(OarSetId),
}

#[derive(Debug, Deserialize)]
pub(crate) struct OarSetFormInput {
    pub(crate) name: String,
    pub(crate) oar_count: String,
    #[serde(default)]
    pub(crate) notes: String,
}

pub(crate) struct OarSetFormData {
    pub(crate) name: String,
    pub(crate) oar_count: String,
    pub(crate) notes: String,
}

impl OarSetFormData {
    pub(crate) fn empty() -> Self {
        Self {
            name: String::new(),
            oar_count: "8".into(),
            notes: String::new(),
        }
    }

    fn from_input(input: &OarSetFormInput) -> Self {
        Self {
            name: input.name.clone(),
            oar_count: input.oar_count.clone(),
            notes: input.notes.clone(),
        }
    }

    fn from_oar_set(os: &OarSet) -> Self {
        Self {
            name: os.name.clone(),
            oar_count: os.oar_count.to_string(),
            notes: os.notes.clone().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PreferencesInput {
    #[serde(default)]
    pub(crate) boat_ids: Vec<String>,
}

struct ParsedOarSet {
    name: String,
    oar_count: i32,
    notes: Option<String>,
}

fn parse_input(input: &OarSetFormInput) -> Result<ParsedOarSet, String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("oar set name is required".into());
    }

    let oar_count: i32 = input
        .oar_count
        .trim()
        .parse()
        .map_err(|_| "oar count must be a number".to_string())?;
    if oar_count < 1 {
        return Err("oar count must be at least 1".into());
    }

    let notes = {
        let t = input.notes.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    };

    Ok(ParsedOarSet {
        name,
        oar_count,
        notes,
    })
}

/// `GET /oars/pick?practice_id=&boat_id=` — oar set picker modal for a boat.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn pick_handler(
    Extension(tenant): Extension<TenantContext>,
    axum::extract::Query(params): axum::extract::Query<PickParams>,
) -> Result<Html<String>, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;

    let practice_id = params.practice_id;
    let boat_id = params.boat_id;

    let (boat, oar_sets, assignments) = tenant
        .db
        .with_conn(move |conn| {
            let boat = lineup_db::boat::Boat::get(conn, boat_id)?
                .ok_or(diesel::result::Error::NotFound)?;
            let oar_sets = OarSet::list_active(conn)?;
            let assignments = lineup_db::oar_set::PracticeBoatOars::list_for_practice_with_names(
                conn,
                practice_id,
            )?;
            Ok((boat, oar_sets, assignments))
        })
        .await
        .map_err(internal_error)?;

    let boats = tenant
        .db
        .with_conn(lineup_db::boat::Boat::list_in_service)
        .await
        .map_err(internal_error)?;

    let content =
        templates::oar_sets::pick_modal(practice_id, &boat, &oar_sets, &assignments, &boats);
    Ok(Html(content.into_string()))
}

#[derive(Debug, Deserialize)]
pub(crate) struct PickParams {
    pub(crate) practice_id: lineup_db::practice::PracticeId,
    pub(crate) boat_id: lineup_db::boat::types::BoatId,
}

/// `POST /oars/auto-assign` — greedy auto-assignment of oar sets to boats.
///
/// Clears existing assignments, then assigns oars greedily: boats sorted
/// by oar demand descending (8+s first), each gets the highest-priority
/// preferred set that has enough remaining oars. Falls back to any set
/// with enough oars if no preferred set fits.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn auto_assign_handler(
    Extension(tenant): Extension<TenantContext>,
    crate::extract::HtmlForm(input): crate::extract::HtmlForm<AutoAssignInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::Coach)?;

    let practice_id = input.practice_id;
    let boat_ids = input.boat_ids;

    tenant
        .db
        .with_conn(move |conn| {
            lineup_db::oar_set::PracticeBoatOars::auto_assign(conn, practice_id, &boat_ids)
        })
        .await
        .map_err(internal_error)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub(crate) struct AutoAssignInput {
    pub(crate) practice_id: lineup_db::practice::PracticeId,
    #[serde(default)]
    pub(crate) boat_ids: Vec<lineup_db::boat::types::BoatId>,
}

/// `POST /oars/assign` — assign or unassign an oar set to a boat for a practice.
#[tracing::instrument(level = "debug", skip_all, err)]
pub(crate) async fn assign_handler(
    Extension(tenant): Extension<TenantContext>,
    Form(input): Form<AssignInput>,
) -> Result<impl IntoResponse, ErrorResponse> {
    crate::handlers::users::require_at_least_role(&tenant.claims, Role::ProgramDirector)?;

    let practice_id = input
        .practice_id
        .parse::<lineup_db::practice::PracticeId>()
        .map_err(|_| {
            ErrorResponse(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid practice_id".into(),
            )
        })?;
    let boat_id = input
        .boat_id
        .parse::<lineup_db::boat::types::BoatId>()
        .map_err(|_| {
            ErrorResponse(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid boat_id".into(),
            )
        })?;

    if input.oar_set_id.trim().is_empty() {
        // Unassign
        tenant
            .db
            .with_conn(move |conn| {
                lineup_db::oar_set::PracticeBoatOars::unassign(conn, practice_id, boat_id)
            })
            .await
            .map_err(internal_error)?;
    } else {
        let oar_set_id = input.oar_set_id.parse::<OarSetId>().map_err(|_| {
            ErrorResponse(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid oar_set_id".into(),
            )
        })?;
        tenant
            .db
            .with_conn(move |conn| {
                lineup_db::oar_set::PracticeBoatOars::assign(conn, practice_id, boat_id, oar_set_id)
            })
            .await
            .map_err(internal_error)?;
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub(crate) struct AssignInput {
    pub(crate) practice_id: String,
    pub(crate) boat_id: String,
    #[serde(default)]
    pub(crate) oar_set_id: String,
}

async fn load(db: &Db, id: OarSetId) -> Result<OarSet, ErrorResponse> {
    let maybe = db
        .with_conn(move |conn| OarSet::get(conn, id))
        .await
        .map_err(internal_error)?;
    maybe.ok_or_else(|| not_found("Oar set not found."))
}
