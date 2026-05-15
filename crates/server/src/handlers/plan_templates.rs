//! Practice plan template handlers.
//!
//! Template timeline editing uses the same editor UI as practice plans.
//! Mutation handlers mirror those in `timeline.rs` but route through
//! `/admin/plan-templates/{id}/timeline/...` and load/save from the
//! `practice_plan_template` table instead of `practice`.

use axum::{extract::Path, http::HeaderMap, response::Html, Extension, Form};
use axum_htmx::HxRequest;
use lineup_db::{
    app_user::Role,
    plan_template::{self, NewPlanTemplate, PlanTemplate, PlanTemplateId},
    timeline::Timeline,
};
use serde::Deserialize;

use crate::templates::layout::{tab_swap, tabbed_section};
use crate::{
    handlers::users::require_at_least_role,
    handlers::{self, internal_error, ErrorResponse},
    state::TenantContext,
    templates,
};

use super::timeline::{practice_timeline_url, ModifierForm, TimelineForm};

fn next_template_name(existing: &[PlanTemplate]) -> String {
    let names: Vec<&str> = existing.iter().map(|t| t.name.as_str()).collect();
    next_available_name(&names)
}

fn next_available_name(names: &[&str]) -> String {
    let base = "new-template";
    if !names.contains(&base) {
        return base.to_string();
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}");
        if !names.iter().any(|&n| n == candidate) {
            return candidate;
        }
    }
    unreachable!()
}

fn template_timeline_url(id: PlanTemplateId) -> String {
    format!("/admin/plan-templates/{id}/timeline")
}

// ── CRUD handlers ────────────────────────────────────────────────────

/// `GET /admin/plan-templates` — list all templates (admin tab).
pub(crate) async fn list_handler(
    Extension(tenant): Extension<TenantContext>,
    hx: HxRequest,
    headers: HeaderMap,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    let tab_content = list_content(&tenant).await?;

    if super::admin::is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(
                super::admin::TABS,
                "plan-templates",
                super::admin::TARGET,
                tenant.claims.role(),
                tab_content,
            )
            .into_string(),
        ));
    }
    let page = tabbed_section(
        super::admin::TABS,
        "plan-templates",
        super::admin::TARGET,
        tenant.claims.role(),
        tab_content,
    );
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

pub(crate) async fn list_content_for(
    tenant: &TenantContext,
) -> Result<maud::Markup, ErrorResponse> {
    list_content(tenant).await
}

async fn list_content(tenant: &TenantContext) -> Result<maud::Markup, ErrorResponse> {
    let templates = tenant
        .db
        .with_conn(PlanTemplate::list)
        .await
        .map_err(internal_error)?;
    Ok(templates::plan_templates::list_content(&templates))
}

/// `POST /admin/plan-templates` — create a new template with auto-generated name.
pub(crate) async fn create_handler(
    Extension(tenant): Extension<TenantContext>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    let author_id = tenant.claims.user_id();
    let tl = Timeline::default_empty(90);
    let timeline_json = tl.to_json();
    let existing = tenant
        .db
        .with_conn(PlanTemplate::list)
        .await
        .map_err(internal_error)?;
    let name = next_template_name(&existing);
    let tmpl = tenant
        .db
        .with_conn(move |conn| {
            PlanTemplate::create(
                conn,
                NewPlanTemplate {
                    name,
                    description: String::new(),
                    author_id,
                    timeline_json,
                },
            )
        })
        .await
        .map_err(internal_error)?;
    let all_cats = tenant
        .db
        .with_conn(plan_template::all_categories)
        .await
        .map_err(internal_error)?;
    let tab_content = templates::plan_templates::detail_content(
        &tmpl,
        &[],
        &all_cats,
        templates::timeline::PlanEditorState::Closed,
    );
    Ok(Html(
        tab_swap(
            super::admin::TABS,
            "plan-templates",
            super::admin::TARGET,
            tenant.claims.role(),
            tab_content,
        )
        .into_string(),
    ))
}

/// `GET /admin/plan-templates/{id}` — template detail / editor (admin tab).
pub(crate) async fn detail_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(template_id): Path<PlanTemplateId>,
    hx: HxRequest,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<super::timeline::EditorQuery>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    // Template editor defaults to open with preview when no param is specified.
    let editor_state = match query.state() {
        templates::timeline::PlanEditorState::Closed => {
            templates::timeline::PlanEditorState::OpenPreview
        }
        s => s,
    };
    let tab_content = detail_tab_content(&tenant, template_id, editor_state).await?;

    if super::admin::is_tab_swap(&headers) {
        return Ok(Html(
            tab_swap(
                super::admin::TABS,
                "plan-templates",
                super::admin::TARGET,
                tenant.claims.role(),
                tab_content,
            )
            .into_string(),
        ));
    }
    let page = tabbed_section(
        super::admin::TABS,
        "plan-templates",
        super::admin::TARGET,
        tenant.claims.role(),
        tab_content,
    );
    Ok(handlers::maybe_page_authed("Admin", page, hx, &tenant))
}

async fn detail_tab_content(
    tenant: &TenantContext,
    template_id: PlanTemplateId,
    editor_state: templates::timeline::PlanEditorState,
) -> Result<maud::Markup, ErrorResponse> {
    let tmpl = tenant
        .db
        .with_conn(move |conn| PlanTemplate::get(conn, template_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error(diesel::result::Error::NotFound))?;
    let tmpl_cats = tenant
        .db
        .with_conn(move |conn| plan_template::categories_for(conn, template_id))
        .await
        .map_err(internal_error)?;
    let all_cats = tenant
        .db
        .with_conn(plan_template::all_categories)
        .await
        .map_err(internal_error)?;
    Ok(templates::plan_templates::detail_content(
        &tmpl,
        &tmpl_cats,
        &all_cats,
        editor_state,
    ))
}

/// `POST /admin/plan-templates/{id}/meta` — update name, description, and categories.
#[derive(Debug, Deserialize)]
pub(crate) struct MetaForm {
    name: String,
    #[serde(default)]
    description: String,
    /// Comma-separated category names.
    #[serde(default)]
    categories: String,
}

pub(crate) async fn update_meta_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(template_id): Path<PlanTemplateId>,
    Form(input): Form<MetaForm>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    let cat_names: Vec<String> = input
        .categories
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    tenant
        .db
        .with_conn(move |conn| {
            PlanTemplate::update_meta(conn, template_id, &input.name, &input.description)?;
            plan_template::set_categories_by_name(conn, template_id, &cat_names)?;
            Ok::<_, diesel::result::Error>(())
        })
        .await
        .map_err(internal_error)?;
    // Re-fetch and render.
    let tmpl = tenant
        .db
        .with_conn(move |conn| PlanTemplate::get(conn, template_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error(diesel::result::Error::NotFound))?;
    let tmpl_cats = tenant
        .db
        .with_conn(move |conn| plan_template::categories_for(conn, template_id))
        .await
        .map_err(internal_error)?;
    let all_cats = tenant
        .db
        .with_conn(plan_template::all_categories)
        .await
        .map_err(internal_error)?;
    Ok(Html(
        templates::plan_templates::meta_section(&tmpl, &tmpl_cats, &all_cats).into_string(),
    ))
}

/// `POST /admin/plan-templates/{id}/delete` — delete and swap back to list.
pub(crate) async fn delete_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(template_id): Path<PlanTemplateId>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    tenant
        .db
        .with_conn(move |conn| PlanTemplate::delete(conn, template_id))
        .await
        .map_err(internal_error)?;
    let tab_content = list_content(&tenant).await?;
    Ok(Html(
        tab_swap(
            super::admin::TABS,
            "plan-templates",
            super::admin::TARGET,
            tenant.claims.role(),
            tab_content,
        )
        .into_string(),
    ))
}

/// `POST /admin/plan-templates/{id}/duplicate` — duplicate and show the copy.
pub(crate) async fn duplicate_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(template_id): Path<PlanTemplateId>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    let original = tenant
        .db
        .with_conn(move |conn| PlanTemplate::get(conn, template_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error(diesel::result::Error::NotFound))?;
    let new_name = format!("{} (copy)", original.name);
    let tmpl = tenant
        .db
        .with_conn(move |conn| PlanTemplate::duplicate(conn, template_id, new_name))
        .await
        .map_err(internal_error)?;
    let tab_content = detail_tab_content(
        &tenant,
        tmpl.id,
        templates::timeline::PlanEditorState::Closed,
    )
    .await?;
    Ok(Html(
        tab_swap(
            super::admin::TABS,
            "plan-templates",
            super::admin::TARGET,
            tenant.claims.role(),
            tab_content,
        )
        .into_string(),
    ))
}

// ── Import into practice ─────────────────────────────────────────────

/// `POST /practices/{id}/import-template` — replace practice timeline with template.
#[derive(Debug, Deserialize)]
pub(crate) struct ImportForm {
    template_id: PlanTemplateId,
}

pub(crate) async fn import_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<lineup_db::practice::PracticeId>,
    Form(input): Form<ImportForm>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    let tmpl = tenant
        .db
        .with_conn(move |conn| PlanTemplate::get(conn, input.template_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error(diesel::result::Error::NotFound))?;
    let tl = tmpl
        .timeline()
        .unwrap_or_else(|| Timeline::default_empty(90));
    let tl2 = tl.clone();
    // Replace practice timeline.
    tenant
        .db
        .with_conn(move |conn| {
            lineup_db::practice::Practice::update_timeline(conn, practice_id, Some(&tl2))
        })
        .await
        .map_err(internal_error)?;
    // Clear plan_dismissed.
    tenant
        .db
        .with_conn(move |conn| {
            lineup_db::practice::Practice::set_plan_dismissed(conn, practice_id, false)
        })
        .await
        .map_err(internal_error)?;
    let base_url = practice_timeline_url(practice_id);
    let import_url = format!("/practices/{practice_id}/import-template");
    Ok(Html(
        templates::timeline::summary_content(&tl, &base_url, Some(&import_url), None).into_string(),
    ))
}

/// `GET /practices/{id}/import-template` — template picker modal.
pub(crate) async fn import_picker_handler(
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<lineup_db::practice::PracticeId>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    let practice = tenant
        .db
        .with_conn(move |conn| lineup_db::practice::Practice::get(conn, practice_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error(diesel::result::Error::NotFound))?;
    let templates = tenant
        .db
        .with_conn(PlanTemplate::list)
        .await
        .map_err(internal_error)?;
    let has_timeline = practice.timeline_json.is_some();
    Ok(Html(
        templates::plan_templates::import_picker_modal(&templates, practice_id, has_timeline)
            .into_string(),
    ))
}

// ── Template timeline mutation handlers ──────────────────────────────
//
// These mirror the practice timeline handlers in `timeline.rs` but operate
// under `/admin/plan-templates/{id}/timeline/...`.

/// `GET /admin/plan-templates/{id}/timeline/edit` — open timeline editor.
pub(crate) async fn tl_open_editor(
    Extension(tenant): Extension<TenantContext>,
    Path(template_id): Path<PlanTemplateId>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    let tmpl = tenant
        .db
        .with_conn(move |conn| PlanTemplate::get(conn, template_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error(diesel::result::Error::NotFound))?;
    let timeline = tmpl
        .timeline()
        .unwrap_or_else(|| Timeline::default_empty(90));
    let base_url = template_timeline_url(template_id);
    Ok(Html(
        templates::timeline::editor_content(
            &timeline,
            &base_url,
            None,
            templates::timeline::PlanEditorState::OpenPreview,
        )
        .into_string(),
    ))
}

/// `POST /admin/plan-templates/{id}/timeline/save` — persist timeline to template.
pub(crate) async fn tl_save(
    Extension(tenant): Extension<TenantContext>,
    Path(template_id): Path<PlanTemplateId>,
    Form(input): Form<TimelineForm>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    let tl = input.parse();
    let tl_for_db = tl.clone();
    tenant
        .db
        .with_conn(move |conn| PlanTemplate::update_timeline(conn, template_id, &tl_for_db))
        .await
        .map_err(internal_error)?;
    let base_url = template_timeline_url(template_id);
    Ok(Html(
        templates::timeline::summary_content(&tl, &base_url, None, None).into_string(),
    ))
}

/// `POST /admin/plan-templates/{id}/timeline/close` — close editor (reload from DB).
pub(crate) async fn tl_close(
    Extension(tenant): Extension<TenantContext>,
    Path(template_id): Path<PlanTemplateId>,
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    let tmpl = tenant
        .db
        .with_conn(move |conn| PlanTemplate::get(conn, template_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error(diesel::result::Error::NotFound))?;
    let timeline = tmpl
        .timeline()
        .unwrap_or_else(|| Timeline::default_empty(90));
    let base_url = template_timeline_url(template_id);
    Ok(Html(
        templates::timeline::summary_content(&timeline, &base_url, None, None).into_string(),
    ))
}

// ── Pure mutation handlers (delegate to shared apply_* functions in timeline.rs) ──

macro_rules! tl_mutation_handler {
    ($name:ident, $form_ty:ty, $apply:path) => {
        pub(crate) async fn $name(
            Path(template_id): Path<PlanTemplateId>,
            Form(input): Form<$form_ty>,
        ) -> Html<String> {
            $apply(input, &template_timeline_url(template_id))
        }
    };
}

use super::timeline::{
    apply_add_block, apply_delete_block, apply_duplicate_block, apply_group_add_segment,
    apply_group_delete_segment, apply_group_patch, apply_group_reorder_segment, apply_group_split,
    apply_insert_template, apply_modifier_add, apply_modifier_override, apply_modifier_remove,
    apply_modifier_revert, apply_modifier_toggle, apply_modifier_update, apply_patch_block,
    apply_patch_segment, apply_reorder_block, apply_update_target, AddForm, DeleteForm,
    DuplicateForm, GroupAddForm, GroupDeleteForm, GroupPatchForm, GroupReorderForm, GroupSplitForm,
    PatchBlockForm, PatchSegmentForm, ReorderForm, TargetForm, TemplateForm,
};

tl_mutation_handler!(tl_add_block, AddForm, apply_add_block);
tl_mutation_handler!(tl_delete_block, DeleteForm, apply_delete_block);
tl_mutation_handler!(tl_patch_block, PatchBlockForm, apply_patch_block);
tl_mutation_handler!(tl_patch_segment, PatchSegmentForm, apply_patch_segment);
tl_mutation_handler!(tl_update_target, TargetForm, apply_update_target);
tl_mutation_handler!(tl_reorder_block, ReorderForm, apply_reorder_block);
tl_mutation_handler!(tl_duplicate_block, DuplicateForm, apply_duplicate_block);
tl_mutation_handler!(tl_group_patch, GroupPatchForm, apply_group_patch);
tl_mutation_handler!(tl_group_add_segment, GroupAddForm, apply_group_add_segment);
tl_mutation_handler!(
    tl_group_delete_segment,
    GroupDeleteForm,
    apply_group_delete_segment
);
tl_mutation_handler!(tl_insert_template, TemplateForm, apply_insert_template);
tl_mutation_handler!(
    tl_group_reorder_segment,
    GroupReorderForm,
    apply_group_reorder_segment
);
tl_mutation_handler!(tl_group_split, GroupSplitForm, apply_group_split);
tl_mutation_handler!(tl_modifier_add, ModifierForm, apply_modifier_add);
tl_mutation_handler!(tl_modifier_remove, ModifierForm, apply_modifier_remove);
tl_mutation_handler!(tl_modifier_update, ModifierForm, apply_modifier_update);
tl_mutation_handler!(tl_modifier_toggle, ModifierForm, apply_modifier_toggle);
tl_mutation_handler!(tl_modifier_override, ModifierForm, apply_modifier_override);
tl_mutation_handler!(tl_modifier_revert, ModifierForm, apply_modifier_revert);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_name_empty() {
        assert_eq!(next_available_name(&[]), "new-template");
    }

    #[test]
    fn next_name_no_collision() {
        assert_eq!(next_available_name(&["race day", "steady"]), "new-template");
    }

    #[test]
    fn next_name_first_collision() {
        assert_eq!(next_available_name(&["new-template"]), "new-template-2");
    }

    #[test]
    fn next_name_multiple_collisions() {
        assert_eq!(
            next_available_name(&["new-template", "new-template-2", "new-template-3"]),
            "new-template-4"
        );
    }

    #[test]
    fn next_name_gap() {
        assert_eq!(
            next_available_name(&["new-template", "new-template-3"]),
            "new-template-2"
        );
    }
}
