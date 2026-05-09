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
    timeline::{
        self, Block, BlockType, Duration, DurationUnit, Group, GroupType, HandDrill, Intensity,
        PausePoint, RotatePer, Rotation, Segment, SegmentType, Slide, Timeline, TimelineItem,
    },
};
use serde::Deserialize;

use crate::templates::layout::{tab_swap, tabbed_section};
use crate::{
    handlers::users::require_at_least_role,
    handlers::{self, internal_error, ErrorResponse},
    state::TenantContext,
    templates,
};

use super::timeline::{default_segment, make_id, practice_timeline_url, CommaSep, TimelineForm};

fn next_template_name(existing: &[PlanTemplate]) -> String {
    let base = "new-template";
    if !existing.iter().any(|t| t.name == base) {
        return base.to_string();
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}");
        if !existing.iter().any(|t| t.name == candidate) {
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
    let tab_content = templates::plan_templates::detail_content(&tmpl, &[], &all_cats);
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
) -> Result<Html<String>, ErrorResponse> {
    require_at_least_role(&tenant.claims, Role::Coach)?;
    let tab_content = detail_tab_content(&tenant, template_id).await?;

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
        &tmpl, &tmpl_cats, &all_cats,
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
    let tab_content = detail_tab_content(&tenant, tmpl.id).await?;
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
        templates::timeline::summary_with_import(&tl, &base_url, &import_url).into_string(),
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
        templates::timeline::editor(&timeline, &base_url, None).into_string(),
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
        templates::timeline::summary(&tl, &base_url).into_string(),
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
        templates::timeline::summary(&timeline, &base_url).into_string(),
    ))
}

// ── Pure mutation handlers (same logic as timeline.rs, different base_url) ──

macro_rules! tl_mutation_handler {
    ($name:ident, $form_ty:ty, $body:expr) => {
        pub(crate) async fn $name(
            Path(template_id): Path<PlanTemplateId>,
            Form(input): Form<$form_ty>,
        ) -> Html<String> {
            let base_url = template_timeline_url(template_id);
            #[allow(clippy::redundant_closure_call)]
            ($body)(input, &base_url)
        }
    };
}

// ── Add block ──

#[derive(Debug, Deserialize)]
pub(crate) struct AddForm {
    #[serde(flatten)]
    base: TimelineForm,
    add_type: String,
}

tl_mutation_handler!(tl_add_block, AddForm, |input: AddForm, base_url: &str| {
    let mut tl = input.base.parse();
    let (new_item, select_id) = match input.add_type.as_str() {
        "rest" => {
            let id = make_id();
            let sid = id.clone();
            (
                TimelineItem::Block(Block {
                    id,
                    block_type: BlockType::Rest,
                    duration: Duration {
                        value: 3.0,
                        unit: DurationUnit::Min,
                    },
                    note: String::new(),
                }),
                sid,
            )
        }
        "turn" => {
            let id = make_id();
            let sid = id.clone();
            (
                TimelineItem::Block(Block {
                    id,
                    block_type: BlockType::Turn,
                    duration: Duration {
                        value: 5.0,
                        unit: DurationUnit::Min,
                    },
                    note: String::new(),
                }),
                sid,
            )
        }
        "warmup" => {
            let g = Group {
                id: make_id(),
                group_type: GroupType::Warmup,
                name: String::new(),
                segments: vec![default_segment()],
                repeat: None,
                rotation: Rotation::default(),
                note: String::new(),
            };
            let sid = g.id.clone();
            (TimelineItem::Group(g), sid)
        }
        "piece" => {
            let g = Group {
                id: make_id(),
                group_type: GroupType::Piece,
                name: String::new(),
                segments: vec![default_segment()],
                repeat: None,
                rotation: Rotation::default(),
                note: String::new(),
            };
            let sid = g.id.clone();
            (TimelineItem::Group(g), sid)
        }
        _ => {
            return Html(
                templates::timeline::editor(&tl, base_url, input.base.selected.as_deref())
                    .into_string(),
            )
        }
    };
    tl.insert_before_dock(vec![new_item]);
    Html(templates::timeline::editor(&tl, base_url, Some(&select_id)).into_string())
});

// ── Delete block ──

#[derive(Debug, Deserialize)]
pub(crate) struct DeleteForm {
    #[serde(flatten)]
    base: TimelineForm,
    delete_id: String,
}

tl_mutation_handler!(
    tl_delete_block,
    DeleteForm,
    |input: DeleteForm, base_url: &str| {
        let mut tl = input.base.parse();
        tl.items
            .retain(|it| it.is_structural() || it.id() != input.delete_id);
        Html(templates::timeline::editor(&tl, base_url, None).into_string())
    }
);

// ── Patch block ──

#[derive(Debug, Deserialize)]
pub(crate) struct PatchBlockForm {
    #[serde(flatten)]
    base: TimelineForm,
    patch_id: String,
    #[serde(default)]
    duration_value: Option<f64>,
    #[serde(default)]
    duration_unit: Option<DurationUnit>,
    #[serde(default)]
    note: Option<String>,
}

tl_mutation_handler!(
    tl_patch_block,
    PatchBlockForm,
    |input: PatchBlockForm, base_url: &str| {
        let mut tl = input.base.parse();
        for item in &mut tl.items {
            if let TimelineItem::Block(b) = item {
                if b.id == input.patch_id {
                    if let Some(v) = input.duration_value {
                        b.duration.value = v.max(0.0);
                    }
                    if let Some(u) = input.duration_unit {
                        b.duration.unit = u;
                    }
                    if let Some(ref n) = input.note {
                        b.note = n.clone();
                    }
                    break;
                }
            }
        }
        Html(
            templates::timeline::editor_no_animate(&tl, base_url, Some(&input.patch_id))
                .into_string(),
        )
    }
);

// ── Patch segment ──

#[derive(Debug, Deserialize)]
pub(crate) struct PatchSegmentForm {
    #[serde(flatten)]
    base: TimelineForm,
    group_id: String,
    segment_id: String,
    #[serde(default)]
    duration_value: Option<f64>,
    #[serde(default)]
    duration_unit: Option<DurationUnit>,
    #[serde(default)]
    rate_low: Option<u8>,
    #[serde(default)]
    rate_high: Option<u8>,
    #[serde(default)]
    intensity: Option<Intensity>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    partial: Option<Slide>,
    #[serde(default)]
    blade: Option<String>,
    #[serde(default)]
    pause_points: CommaSep<PausePoint>,
    #[serde(default)]
    pause_every: Option<u32>,
    #[serde(default)]
    drills: CommaSep<HandDrill>,
    #[serde(default)]
    _range_toggle: Option<String>,
    #[serde(default)]
    seg_type: Option<SegmentType>,
}

tl_mutation_handler!(
    tl_patch_segment,
    PatchSegmentForm,
    |mut input: PatchSegmentForm, base_url: &str| {
        let mut tl = input.base.parse();
        for item in &mut tl.items {
            if let TimelineItem::Group(g) = item {
                if g.id == input.group_id {
                    for s in &mut g.segments {
                        if s.id == input.segment_id {
                            if let Some(v) = input.duration_value {
                                s.duration.value = v.max(0.0);
                            }
                            if let Some(u) = input.duration_unit {
                                s.duration.unit = u;
                            }
                            if input.rate_low.is_some() || input.rate_high.is_some() {
                                let cur = s.rate.unwrap_or([20, 20]);
                                let lo = input.rate_low.unwrap_or(cur[0]).clamp(10, 50);
                                let mut hi = input.rate_high.unwrap_or(cur[1]).clamp(lo, 50);
                                if input._range_toggle.is_none() {
                                    hi = lo;
                                }
                                s.rate = Some([lo, hi]);
                            }
                            if let Some(int) = input.intensity {
                                s.intensity = Some(int);
                            }
                            if let Some(ref n) = input.note {
                                s.note = n.clone();
                            }
                            if let Some(sl) = input.partial {
                                s.partial = Some(sl);
                            }
                            if let Some(ref bl) = input.blade {
                                s.blade = match bl.as_str() {
                                    "square" => Some(timeline::Blade::Square),
                                    "partial-feather" => Some(timeline::Blade::PartialFeather),
                                    "none" => None,
                                    _ => Some(timeline::Blade::Feather),
                                };
                            }
                            s.pause = std::mem::take(&mut input.pause_points.0);
                            s.pause_every = input.pause_every.filter(|&n| n > 0);
                            s.drills = std::mem::take(&mut input.drills.0);
                            if let Some(st) = input.seg_type {
                                s.seg_type = st;
                            }
                            break;
                        }
                    }
                    break;
                }
            }
        }
        Html(
            templates::timeline::editor_no_animate(&tl, base_url, Some(&input.segment_id))
                .into_string(),
        )
    }
);

// ── Update target ──

#[derive(Debug, Deserialize)]
pub(crate) struct TargetForm {
    #[serde(flatten)]
    base: TimelineForm,
    new_target: u32,
}

tl_mutation_handler!(
    tl_update_target,
    TargetForm,
    |input: TargetForm, base_url: &str| {
        let mut tl = input.base.parse();
        tl.target_minutes = input.new_target.clamp(20, 240);
        Html(
            templates::timeline::editor_no_animate(&tl, base_url, input.base.selected.as_deref())
                .into_string(),
        )
    }
);

// ── Reorder block ──

#[derive(Debug, Deserialize)]
pub(crate) struct ReorderForm {
    #[serde(flatten)]
    base: TimelineForm,
    drag_id: String,
    drop_before_id: String,
}

tl_mutation_handler!(
    tl_reorder_block,
    ReorderForm,
    |input: ReorderForm, base_url: &str| {
        let mut tl = input.base.parse();
        let drag_idx = tl
            .items
            .iter()
            .position(|it| it.id() == input.drag_id && !it.is_structural());
        if let Some(idx) = drag_idx {
            let item = tl.items.remove(idx);
            let drop_idx = if input.drop_before_id == "__end__" {
                tl.items.iter().position(|it| matches!(it, TimelineItem::Block(b) if b.block_type == BlockType::Dock)).unwrap_or(tl.items.len())
            } else {
                tl.items
                    .iter()
                    .position(|it| it.id() == input.drop_before_id)
                    .unwrap_or(tl.items.len())
            };
            let dock_idx = tl
                .items
                .iter()
                .position(
                    |it| matches!(it, TimelineItem::Block(b) if b.block_type == BlockType::Dock),
                )
                .unwrap_or(tl.items.len());
            let drop_idx = drop_idx.min(dock_idx);
            let launch_end = tl
                .items
                .iter()
                .position(
                    |it| !matches!(it, TimelineItem::Block(b) if b.block_type == BlockType::Launch),
                )
                .unwrap_or(0);
            let drop_idx = drop_idx.max(launch_end);
            tl.items.insert(drop_idx, item);
        }
        Html(templates::timeline::editor(&tl, base_url, Some(&input.drag_id)).into_string())
    }
);

// ── Duplicate block ──

#[derive(Debug, Deserialize)]
pub(crate) struct DuplicateForm {
    #[serde(flatten)]
    base: TimelineForm,
    dup_id: String,
}

tl_mutation_handler!(
    tl_duplicate_block,
    DuplicateForm,
    |input: DuplicateForm, base_url: &str| {
        let mut tl = input.base.parse();
        if let Some(idx) = tl.items.iter().position(|it| it.id() == input.dup_id) {
            let item = &tl.items[idx];
            if !item.is_structural() {
                let mut dup = item.clone();
                let new_id = make_id();
                match &mut dup {
                    TimelineItem::Block(b) => b.id = new_id.clone(),
                    TimelineItem::Group(g) => {
                        g.id = new_id.clone();
                        for s in &mut g.segments {
                            s.id = make_id();
                        }
                    }
                }
                tl.items.insert(idx + 1, dup);
                return Html(
                    templates::timeline::editor(&tl, base_url, Some(&new_id)).into_string(),
                );
            }
        }
        Html(
            templates::timeline::editor(&tl, base_url, input.base.selected.as_deref())
                .into_string(),
        )
    }
);

// ── Group patch ──

#[derive(Debug, Deserialize)]
pub(crate) struct GroupPatchForm {
    #[serde(flatten)]
    base: TimelineForm,
    group_id: String,
    #[serde(default)]
    group_name: Option<String>,
    #[serde(default)]
    repeat: Option<String>,
    #[serde(default)]
    row_by: Option<String>,
    #[serde(default)]
    rotate_by: Option<String>,
    #[serde(default)]
    rotate_per: Option<String>,
    #[serde(default)]
    rotate_per_value: Option<String>,
    #[serde(default)]
    rotate_per_unit: Option<String>,
    #[serde(default)]
    rotations: Option<String>,
    #[serde(default)]
    group_note: Option<String>,
    #[serde(default)]
    group_type: Option<GroupType>,
    #[serde(default)]
    prev_seg_type: Option<SegmentType>,
}

tl_mutation_handler!(
    tl_group_patch,
    GroupPatchForm,
    |input: GroupPatchForm, base_url: &str| {
        let mut tl = input.base.parse();
        for item in &mut tl.items {
            if let TimelineItem::Group(g) = item {
                if g.id == input.group_id {
                    if let Some(ref name) = input.group_name {
                        g.name = name.clone();
                    }
                    if let Some(ref r) = input.repeat {
                        g.repeat = r.parse::<u8>().ok().filter(|&n| n > 1);
                    }
                    if let Some(ref rb) = input.row_by {
                        if rb == "all" {
                            g.rotation = Rotation::default();
                        } else {
                            g.rotation.row_by = rb.parse::<u8>().ok().filter(|&n| n > 0);
                            let old_rotate_by = g.rotation.rotate_by;
                            if let Some(ref rb2) = input.rotate_by {
                                g.rotation.rotate_by = rb2.parse::<u8>().ok().filter(|&n| n > 0);
                            }
                            if g.rotation.rotate_by.is_none() {
                                g.rotation.rotate_by = Some(2);
                            }
                            if g.rotation.rotate_by != old_rotate_by
                                && g.rotation.rotate_per != RotatePer::Group
                            {
                                g.rotation.rotations =
                                    g.rotation.rotate_by.map(|rb| (8 / rb).max(1));
                            }
                            if let Some(ref rp) = input.rotate_per {
                                g.rotation.rotate_per = match rp.as_str() {
                                    "segment" => RotatePer::Segment,
                                    "group" => RotatePer::Group,
                                    "every" => {
                                        let v = input
                                            .rotate_per_value
                                            .as_deref()
                                            .and_then(|s| s.parse::<f64>().ok())
                                            .unwrap_or(10.0);
                                        let u = input
                                            .rotate_per_unit
                                            .as_deref()
                                            .map(|s| {
                                                s.parse::<DurationUnit>()
                                                    .unwrap_or(DurationUnit::Strokes)
                                            })
                                            .unwrap_or(DurationUnit::Strokes);
                                        RotatePer::Every { value: v, unit: u }
                                    }
                                    _ => RotatePer::None,
                                };
                            }
                            if let Some(ref r) = input.rotations {
                                g.rotation.rotations = r.parse::<u8>().ok().filter(|&n| n > 0);
                            }
                            if g.rotation.rotations.is_none() {
                                if let Some(rb) = g.rotation.rotate_by {
                                    g.rotation.rotations = Some((8 / rb).max(1));
                                }
                            }
                        }
                    }
                    if let Some(ref note) = input.group_note {
                        g.note = note.clone();
                    }
                    if let Some(gt) = input.group_type {
                        g.group_type = gt;
                    }
                    break;
                }
            }
        }
        let sel = input.base.selected.as_deref().unwrap_or(&input.group_id);
        let new_seg_type = tl.items.iter().find_map(|it| {
            if let TimelineItem::Group(g) = it {
                g.segments.iter().find(|s| s.id == sel).map(|s| s.seg_type)
            } else {
                None
            }
        });
        let type_changed = match (input.prev_seg_type, new_seg_type) {
            (Some(prev), Some(new_type)) => prev != new_type,
            (None, Some(_)) => true,
            _ => false,
        };
        let render = if type_changed {
            templates::timeline::editor
        } else {
            templates::timeline::editor_no_animate
        };
        Html(render(&tl, base_url, Some(sel)).into_string())
    }
);

// ── Group add segment ──

#[derive(Debug, Deserialize)]
pub(crate) struct GroupAddForm {
    #[serde(flatten)]
    base: TimelineForm,
    group_id: String,
    seg_type: SegmentType,
}

tl_mutation_handler!(
    tl_group_add_segment,
    GroupAddForm,
    |input: GroupAddForm, base_url: &str| {
        let mut tl = input.base.parse();
        let st = input.seg_type;
        let mut seg = default_segment();
        seg.seg_type = st;
        if !st.is_work() {
            seg.rate = None;
            seg.intensity = None;
            seg.duration.value = 3.0;
        }
        let new_id = seg.id.clone();
        for item in &mut tl.items {
            if let TimelineItem::Group(g) = item {
                if g.id == input.group_id {
                    g.segments.push(seg);
                    break;
                }
            }
        }
        Html(templates::timeline::editor(&tl, base_url, Some(&new_id)).into_string())
    }
);

// ── Group delete segment ──

#[derive(Debug, Deserialize)]
pub(crate) struct GroupDeleteForm {
    #[serde(flatten)]
    base: TimelineForm,
    group_id: String,
    segment_id: String,
}

tl_mutation_handler!(
    tl_group_delete_segment,
    GroupDeleteForm,
    |input: GroupDeleteForm, base_url: &str| {
        let mut tl = input.base.parse();
        for item in &mut tl.items {
            if let TimelineItem::Group(g) = item {
                if g.id == input.group_id {
                    g.segments.retain(|s| s.id != input.segment_id);
                    break;
                }
            }
        }
        tl.items
            .retain(|it| !matches!(it, TimelineItem::Group(g) if g.segments.is_empty()));
        Html(templates::timeline::editor(&tl, base_url, Some(&input.group_id)).into_string())
    }
);

// ── Insert built-in template ──

#[derive(Debug, Deserialize)]
pub(crate) struct InsertTemplateForm {
    #[serde(flatten)]
    base: TimelineForm,
    template_id: String,
}

tl_mutation_handler!(
    tl_insert_template,
    InsertTemplateForm,
    |input: InsertTemplateForm, base_url: &str| {
        let mut tl = input.base.parse();
        let templates = timeline::built_in_templates();
        if let Some(tmpl) = templates.iter().find(|t| t.id == input.template_id) {
            let segments: Vec<Segment> = (tmpl.segments)()
                .into_iter()
                .map(|mut s| {
                    s.id = make_id();
                    s
                })
                .collect();
            let g = Group {
                id: make_id(),
                group_type: tmpl.group_type,
                name: tmpl.group_name.to_string(),
                segments,
                repeat: tmpl.repeat,
                rotation: tmpl.rotation.clone(),
                note: String::new(),
            };
            let select_id = g.id.clone();
            tl.insert_before_dock(vec![TimelineItem::Group(g)]);
            return Html(
                templates::timeline::editor(&tl, base_url, Some(&select_id)).into_string(),
            );
        }
        Html(
            templates::timeline::editor(&tl, base_url, input.base.selected.as_deref())
                .into_string(),
        )
    }
);

// ── Group reorder segment ──

#[derive(Debug, Deserialize)]
pub(crate) struct GroupReorderForm {
    #[serde(flatten)]
    base: TimelineForm,
    group_id: String,
    drag_id: String,
    drop_before_id: String,
}

tl_mutation_handler!(
    tl_group_reorder_segment,
    GroupReorderForm,
    |input: GroupReorderForm, base_url: &str| {
        let mut tl = input.base.parse();
        for item in &mut tl.items {
            if let TimelineItem::Group(g) = item {
                if g.id == input.group_id {
                    let drag_idx = g.segments.iter().position(|s| s.id == input.drag_id);
                    if let Some(idx) = drag_idx {
                        let seg = g.segments.remove(idx);
                        let drop_idx = g
                            .segments
                            .iter()
                            .position(|s| s.id == input.drop_before_id)
                            .unwrap_or(g.segments.len());
                        g.segments.insert(drop_idx, seg);
                    }
                    break;
                }
            }
        }
        Html(templates::timeline::editor(&tl, base_url, Some(&input.drag_id)).into_string())
    }
);
