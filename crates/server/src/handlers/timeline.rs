//! Timeline editor handlers.
//!
//! The timeline editor keeps its state in a hidden form field (`timeline`
//! containing JSON).  Each action mutates the timeline and returns the
//! re-rendered editor fragment via HTMX swap.

use axum::{extract::Path, response::Html, Extension, Form};
use lineup_db::{
    practice::{Practice, PracticeId},
    timeline::{
        self, Blade, Block, BlockType, Duration, DurationUnit, Group, GroupType, HandDrill,
        Intensity, Modifier, PausePoint, RotatePer, Rotation, Segment, SegmentType, Slide,
        Timeline, TimelineItem,
    },
};
use serde::Deserialize;

use crate::{handlers::internal_error, state::TenantContext, templates};

/// Shared form field: the current timeline JSON + selected item id.
#[derive(Debug, Deserialize)]
pub(crate) struct TimelineForm {
    #[serde(default)]
    pub timeline: Option<String>,
    #[serde(default)]
    pub selected: Option<String>,
    #[serde(default)]
    pub target_minutes: Option<u32>,
    /// Editor visibility state carried through HTMX round-trips.
    #[serde(default)]
    pub plan_editor: Option<String>,
}

impl TimelineForm {
    pub(crate) fn editor_state(&self) -> templates::timeline::PlanEditorState {
        match self.plan_editor.as_deref() {
            Some("open") => templates::timeline::PlanEditorState::Open,
            Some("open_preview") => templates::timeline::PlanEditorState::OpenPreview,
            _ => templates::timeline::PlanEditorState::Open, // default to Open when in editor forms
        }
    }
}

impl TimelineForm {
    pub(crate) fn parse(&self) -> Timeline {
        let mut tl: Timeline = self
            .timeline
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| Timeline::default_empty(self.target_minutes.unwrap_or(90)));
        // Fix any duplicate IDs from older data.
        dedup_ids(&mut tl);
        tl
    }
}

/// Ensure all item and segment IDs are unique.  Older timelines may
/// have collisions from the timestamp-only `make_id()`.
fn dedup_ids(tl: &mut Timeline) {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    for item in &mut tl.items {
        match item {
            TimelineItem::Block(b) => {
                if !seen.insert(b.id.clone()) {
                    b.id = make_id();
                }
            }
            TimelineItem::Group(g) => {
                if !seen.insert(g.id.clone()) {
                    g.id = make_id();
                }
                for s in &mut g.segments {
                    if !seen.insert(s.id.clone()) {
                        s.id = make_id();
                    }
                }
            }
        }
    }
}

pub(crate) fn make_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u32;
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("blk-{:x}-{:x}", ts, seq)
}

pub(crate) fn practice_timeline_url(practice_id: PracticeId) -> String {
    format!("/practices/{practice_id}/timeline")
}

pub(crate) fn default_segment() -> Segment {
    Segment {
        id: make_id(),
        seg_type: SegmentType::Work,
        duration: Duration {
            value: 5.0,
            unit: DurationUnit::Min,
        },
        rate: Some([20, 20]),
        intensity: Some(timeline::Intensity::Paddle),
        modifiers: vec![],
        note: String::new(),
    }
}

/// Query params for editor state.
#[derive(Debug, Deserialize)]
pub(crate) struct EditorQuery {
    #[serde(default)]
    pub plan_editor: Option<String>,
}

impl EditorQuery {
    pub(crate) fn state(&self) -> templates::timeline::PlanEditorState {
        match self.plan_editor.as_deref() {
            Some("open") => templates::timeline::PlanEditorState::Open,
            Some("open_preview") => templates::timeline::PlanEditorState::OpenPreview,
            _ => templates::timeline::PlanEditorState::Closed,
        }
    }
}

/// `GET /history/{id}/timeline/edit` — open the timeline editor.
pub(crate) async fn open_editor(
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
    axum::extract::Query(query): axum::extract::Query<EditorQuery>,
) -> Result<Html<String>, crate::handlers::ErrorResponse> {
    let practice = tenant
        .db
        .with_conn(move |conn| Practice::get(conn, practice_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error(diesel::result::Error::NotFound))?;

    let timeline = practice.timeline().unwrap_or_else(|| {
        Timeline::default_empty(
            practice
                .duration_minutes
                .map(|d| d.as_int() as u32)
                .unwrap_or(90),
        )
    });

    let base_url = practice_timeline_url(practice_id);
    let html = templates::timeline::editor_content(&timeline, &base_url, None, query.state());
    Ok(Html(html.into_string()))
}

/// `POST /history/{id}/timeline/add` — add a bare block or new group.
#[derive(Debug, Deserialize)]
pub(crate) struct AddForm {
    #[serde(flatten)]
    base: TimelineForm,
    add_type: String,
}

pub(crate) async fn add_block(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<AddForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
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
                modifiers: vec![],
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
                modifiers: vec![],
                note: String::new(),
            };
            let sid = g.id.clone();
            (TimelineItem::Group(g), sid)
        }
        _ => {
            return Html(
                templates::timeline::editor_content(
                    &tl,
                    &base_url,
                    input.base.selected.as_deref(),
                    input.base.editor_state(),
                )
                .into_string(),
            )
        }
    };

    match input.base.selected.as_deref().filter(|s| !s.is_empty()) {
        Some(sel) => tl.insert_after_item(sel, vec![new_item]),
        None => tl.insert_before_dock(vec![new_item]),
    }
    Html(
        templates::timeline::editor_content(
            &tl,
            &base_url,
            Some(&select_id),
            input.base.editor_state(),
        )
        .into_string(),
    )
}

/// `POST /history/{id}/timeline/delete` — remove a block or group.
#[derive(Debug, Deserialize)]
pub(crate) struct DeleteForm {
    #[serde(flatten)]
    base: TimelineForm,
    delete_id: String,
}

pub(crate) async fn delete_block(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<DeleteForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
    let mut tl = input.base.parse();
    tl.items
        .retain(|it| it.is_structural() || it.id() != input.delete_id);
    Html(
        templates::timeline::editor_content(&tl, &base_url, None, input.base.editor_state())
            .into_string(),
    )
}

/// `POST /history/{id}/timeline/patch-block` — update fields on a bare block.
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

pub(crate) async fn patch_block(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<PatchBlockForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
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
        templates::timeline::editor_content(
            &tl,
            &base_url,
            Some(&input.patch_id),
            input.base.editor_state(),
        )
        .into_string(),
    )
}

/// `POST /history/{id}/timeline/patch-segment` — update base fields on a segment inside a group.
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
    _range_toggle: Option<String>,
    #[serde(default)]
    seg_type: Option<SegmentType>,
}

pub(crate) async fn patch_segment(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<PatchSegmentForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
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
                            } else if lo == hi {
                                // Toggling range on: bump hi so two inputs appear
                                hi = (lo + 2).clamp(lo, 50);
                            }
                            s.rate = Some([lo, hi]);
                        }
                        if let Some(int) = input.intensity {
                            s.intensity = Some(int);
                        }
                        if let Some(ref n) = input.note {
                            s.note = n.clone();
                        }
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

    // Keep the segment selected so its editor stays open.
    Html(
        templates::timeline::editor_content(
            &tl,
            &base_url,
            Some(&input.segment_id),
            input.base.editor_state(),
        )
        .into_string(),
    )
}

/// `POST /history/{id}/timeline/target` — update target minutes.
#[derive(Debug, Deserialize)]
pub(crate) struct TargetForm {
    #[serde(flatten)]
    base: TimelineForm,
    new_target: u32,
}

pub(crate) async fn update_target(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<TargetForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
    let mut tl = input.base.parse();
    tl.target_minutes = input.new_target.clamp(20, 240);
    Html(
        templates::timeline::editor_content(
            &tl,
            &base_url,
            input.base.selected.as_deref(),
            input.base.editor_state(),
        )
        .into_string(),
    )
}

/// `POST /history/{id}/timeline/save` — persist to DB.
pub(crate) async fn save_timeline(
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<TimelineForm>,
) -> Result<Html<String>, crate::handlers::ErrorResponse> {
    let base_url = practice_timeline_url(practice_id);
    let import_url = format!("/practices/{practice_id}/import-template");
    let tl = input.parse();
    let tl_for_db = tl.clone();
    tenant
        .db
        .with_conn(move |conn| Practice::update_timeline(conn, practice_id, Some(&tl_for_db)))
        .await
        .map_err(internal_error)?;
    Ok(Html(
        templates::timeline::summary_content(&tl, &base_url, Some(&import_url), None).into_string(),
    ))
}

/// `POST /history/{id}/timeline/close` — close without saving.
pub(crate) async fn close_editor(
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
) -> Result<Html<String>, crate::handlers::ErrorResponse> {
    let base_url = practice_timeline_url(practice_id);
    let import_url = format!("/practices/{practice_id}/import-template");
    let practice = tenant
        .db
        .with_conn(move |conn| Practice::get(conn, practice_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error(diesel::result::Error::NotFound))?;
    let timeline = practice.timeline();
    Ok(Html(
        templates::timeline::summary_content(
            timeline.as_ref().unwrap_or(&Timeline::default_empty(90)),
            &base_url,
            Some(&import_url),
            None,
        )
        .into_string(),
    ))
}

/// `POST /history/{id}/timeline/reorder` — move an item.
#[derive(Debug, Deserialize)]
pub(crate) struct ReorderForm {
    #[serde(flatten)]
    base: TimelineForm,
    drag_id: String,
    drop_before_id: String,
}

pub(crate) async fn reorder_block(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<ReorderForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
    let mut tl = input.base.parse();
    let drag_idx = tl
        .items
        .iter()
        .position(|it| it.id() == input.drag_id && !it.is_structural());
    if let Some(idx) = drag_idx {
        let item = tl.items.remove(idx);
        let drop_idx = if input.drop_before_id == "__end__" {
            // Insert before the dock (last structural item).
            tl.items
                .iter()
                .position(
                    |it| matches!(it, TimelineItem::Block(b) if b.block_type == BlockType::Dock),
                )
                .unwrap_or(tl.items.len())
        } else {
            tl.items
                .iter()
                .position(|it| it.id() == input.drop_before_id)
                .unwrap_or(tl.items.len())
        };
        // Never insert past the dock.
        let dock_idx = tl
            .items
            .iter()
            .position(|it| matches!(it, TimelineItem::Block(b) if b.block_type == BlockType::Dock))
            .unwrap_or(tl.items.len());
        let drop_idx = drop_idx.min(dock_idx);
        // Never insert before launch.
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
    Html(
        templates::timeline::editor_content(
            &tl,
            &base_url,
            Some(&input.drag_id),
            input.base.editor_state(),
        )
        .into_string(),
    )
}

/// `POST /history/{id}/timeline/duplicate` — duplicate an item.
#[derive(Debug, Deserialize)]
pub(crate) struct DuplicateForm {
    #[serde(flatten)]
    base: TimelineForm,
    dup_id: String,
}

pub(crate) async fn duplicate_block(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<DuplicateForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
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
                templates::timeline::editor_content(
                    &tl,
                    &base_url,
                    Some(&new_id),
                    input.base.editor_state(),
                )
                .into_string(),
            );
        }
    }
    Html(
        templates::timeline::editor_content(
            &tl,
            &base_url,
            input.base.selected.as_deref(),
            input.base.editor_state(),
        )
        .into_string(),
    )
}

/// `POST /history/{id}/timeline/group-patch` — update group name/rotation/type.
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
}

pub(crate) async fn group_patch(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<GroupPatchForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
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
                        // Reset entire rotation — ignore any stale
                        // rotate_by/rotate_per/rotations fields that
                        // were in the form before the swap.
                        g.rotation = Rotation::default();
                    } else {
                        g.rotation.row_by = rb.parse::<u8>().ok().filter(|&n| n > 0);
                        let old_rotate_by = g.rotation.rotate_by;
                        if let Some(ref rb2) = input.rotate_by {
                            g.rotation.rotate_by = rb2.parse::<u8>().ok().filter(|&n| n > 0);
                        }
                        // Default rotate_by to 2 when first activating rotation.
                        if g.rotation.rotate_by.is_none() {
                            g.rotation.rotate_by = Some(2);
                        }
                        // Recompute rotations when rotate_by changes, but only
                        // when per-group isn't already showing (avoid overwriting
                        // the coach's manual value while the input is visible).
                        if g.rotation.rotate_by != old_rotate_by
                            && g.rotation.rotate_per != RotatePer::Group
                        {
                            g.rotation.rotations = g.rotation.rotate_by.map(|rb| (8 / rb).max(1));
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
                        // Default rotations to 8 / rotate_by when not explicitly set.
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
    // Use selected from form (may be a segment ID) or fall back to group.
    let sel = input.base.selected.as_deref().unwrap_or(&input.group_id);

    Html(
        templates::timeline::editor_content(&tl, &base_url, Some(sel), input.base.editor_state())
            .into_string(),
    )
}

/// `POST /history/{id}/timeline/group-add` — add a segment to a group.
#[derive(Debug, Deserialize)]
pub(crate) struct GroupAddForm {
    #[serde(flatten)]
    base: TimelineForm,
    group_id: String,
    seg_type: SegmentType,
}

pub(crate) async fn group_add_segment(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<GroupAddForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
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
    Html(
        templates::timeline::editor_content(
            &tl,
            &base_url,
            Some(&new_id),
            input.base.editor_state(),
        )
        .into_string(),
    )
}

/// `POST /history/{id}/timeline/group-delete` — remove a segment from a group.
#[derive(Debug, Deserialize)]
pub(crate) struct GroupDeleteForm {
    #[serde(flatten)]
    base: TimelineForm,
    group_id: String,
    segment_id: String,
}

pub(crate) async fn group_delete_segment(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<GroupDeleteForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
    let mut tl = input.base.parse();
    for item in &mut tl.items {
        if let TimelineItem::Group(g) = item {
            if g.id == input.group_id {
                g.segments.retain(|s| s.id != input.segment_id);
                break;
            }
        }
    }
    // If group has no segments, remove it.
    tl.items
        .retain(|it| !matches!(it, TimelineItem::Group(g) if g.segments.is_empty()));
    Html(
        templates::timeline::editor_content(
            &tl,
            &base_url,
            Some(&input.group_id),
            input.base.editor_state(),
        )
        .into_string(),
    )
}

/// `POST /history/{id}/timeline/template` — insert a built-in template.
#[derive(Debug, Deserialize)]
pub(crate) struct TemplateForm {
    #[serde(flatten)]
    base: TimelineForm,
    template_id: String,
}

pub(crate) async fn insert_template(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<TemplateForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
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
            modifiers: tmpl.modifiers.clone(),
            note: String::new(),
        };
        let select_id = g.id.clone();
        match input.base.selected.as_deref().filter(|s| !s.is_empty()) {
            Some(sel) => tl.insert_after_item(sel, vec![TimelineItem::Group(g)]),
            None => tl.insert_before_dock(vec![TimelineItem::Group(g)]),
        }
        return Html(
            templates::timeline::editor_content(
                &tl,
                &base_url,
                Some(&select_id),
                input.base.editor_state(),
            )
            .into_string(),
        );
    }
    Html(
        templates::timeline::editor_content(
            &tl,
            &base_url,
            input.base.selected.as_deref(),
            input.base.editor_state(),
        )
        .into_string(),
    )
}

/// `POST /history/{id}/timeline/group-reorder` — reorder segments within a group.
#[derive(Debug, Deserialize)]
pub(crate) struct GroupReorderForm {
    #[serde(flatten)]
    base: TimelineForm,
    group_id: String,
    drag_id: String,
    drop_before_id: String,
}

pub(crate) async fn group_reorder_segment(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<GroupReorderForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
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
    Html(
        templates::timeline::editor_content(
            &tl,
            &base_url,
            Some(&input.drag_id),
            input.base.editor_state(),
        )
        .into_string(),
    )
}

/// `POST .../group-split` — expand a repeated group into N independent copies.
#[derive(Debug, Deserialize)]
pub(crate) struct GroupSplitForm {
    #[serde(flatten)]
    pub(crate) base: TimelineForm,
    pub(crate) group_id: String,
}

pub(crate) async fn group_split(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<GroupSplitForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
    let mut tl = input.base.parse();

    for item in &mut tl.items {
        if let TimelineItem::Group(g) = item {
            if g.id == input.group_id {
                let reps = g.repeat.unwrap_or(1).max(1) as usize;
                if reps > 1 {
                    // Duplicate the segment sequence N times with fresh IDs
                    let original_segs = g.segments.clone();
                    for _ in 1..reps {
                        for s in &original_segs {
                            let mut copy = s.clone();
                            copy.id = make_id();
                            g.segments.push(copy);
                        }
                    }
                    g.repeat = None;
                }
                break;
            }
        }
    }

    Html(
        templates::timeline::editor_content(
            &tl,
            &base_url,
            Some(&input.group_id),
            input.base.editor_state(),
        )
        .into_string(),
    )
}

// ── Modifier mutation handlers ──────────────────────────────────────────

/// Shared form for modifier mutations.
#[derive(Debug, Deserialize)]
pub(crate) struct ModifierForm {
    #[serde(flatten)]
    pub(crate) base: TimelineForm,
    pub(crate) group_id: String,
    #[serde(default)]
    pub(crate) segment_id: Option<String>,
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) scope: Option<String>,
    #[serde(default)]
    pub(crate) value: Option<String>,
    #[serde(default)]
    pub(crate) subfield: Option<String>,
    // Repeating emphasis compound fields
    #[serde(default)]
    pub(crate) re_every: Option<u32>,
    #[serde(default)]
    pub(crate) re_every_unit: Option<DurationUnit>,
    #[serde(default)]
    pub(crate) re_count: Option<u32>,
    #[serde(default)]
    pub(crate) re_label: Option<String>,
}

/// Helper: find the modifier list to mutate (group or segment).
pub(crate) fn find_modifier_target<'a>(
    tl: &'a mut Timeline,
    group_id: &str,
    segment_id: Option<&str>,
    scope: Option<&str>,
) -> Option<&'a mut Vec<Modifier>> {
    for item in &mut tl.items {
        if let TimelineItem::Group(g) = item {
            if g.id == group_id {
                if scope == Some("group") {
                    return Some(&mut g.modifiers);
                }
                if let Some(sid) = segment_id {
                    if let Some(s) = g.segments.iter_mut().find(|s| s.id == sid) {
                        return Some(&mut s.modifiers);
                    }
                }
                // Default to group if no segment specified
                return Some(&mut g.modifiers);
            }
        }
    }
    None
}

/// `POST .../modifier-add` — add a modifier with default value.
pub(crate) async fn modifier_add(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<ModifierForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
    let mut tl = input.base.parse();
    if let Some(m) = Modifier::default_for_kind(&input.kind) {
        if let Some(mods) = find_modifier_target(
            &mut tl,
            &input.group_id,
            input.segment_id.as_deref(),
            input.scope.as_deref(),
        ) {
            // Don't add duplicate kinds
            if !mods.iter().any(|existing| existing.kind_id() == input.kind) {
                mods.push(m);
            }
        }
    }
    let sel = input.segment_id.as_deref().unwrap_or(&input.group_id);
    Html(
        templates::timeline::editor_content(&tl, &base_url, Some(sel), input.base.editor_state())
            .into_string(),
    )
}

/// `POST .../modifier-remove` — remove a modifier by kind.
pub(crate) async fn modifier_remove(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<ModifierForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
    let mut tl = input.base.parse();
    if let Some(mods) = find_modifier_target(
        &mut tl,
        &input.group_id,
        input.segment_id.as_deref(),
        input.scope.as_deref(),
    ) {
        mods.retain(|m| m.kind_id() != input.kind);
    }
    let sel = input.segment_id.as_deref().unwrap_or(&input.group_id);
    Html(
        templates::timeline::editor_content(&tl, &base_url, Some(sel), input.base.editor_state())
            .into_string(),
    )
}

/// `POST .../modifier-update` — update a modifier's value (picks-one, free text, pause every).
pub(crate) async fn modifier_update(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<ModifierForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
    let mut tl = input.base.parse();
    if let Some(mods) = find_modifier_target(
        &mut tl,
        &input.group_id,
        input.segment_id.as_deref(),
        input.scope.as_deref(),
    ) {
        if let Some(m) = mods.iter_mut().find(|m| m.kind_id() == input.kind) {
            let val = input.value.as_deref().unwrap_or("");
            match m {
                Modifier::Blade { value } => {
                    *value = match val {
                        "square" => Blade::Square,
                        "partial-feather" => Blade::PartialFeather,
                        _ => Blade::Feather,
                    };
                }
                Modifier::Partial { value } => {
                    if let Ok(v) = val.parse::<Slide>() {
                        *value = v;
                    }
                }
                Modifier::PauseAt { every, .. } => {
                    if input.subfield.as_deref() == Some("every") {
                        *every = val.parse::<u32>().ok().filter(|&n| n > 0);
                    }
                }
                Modifier::Emphasis { text } => {
                    *text = val.to_string();
                }
                Modifier::RepeatingEmphasis {
                    every,
                    every_unit,
                    count,
                    label,
                } => {
                    if let Some(v) = input.re_every {
                        *every = v.max(1);
                    }
                    if let Some(u) = input.re_every_unit {
                        *every_unit = u;
                    }
                    if let Some(c) = input.re_count {
                        *count = c.max(1);
                    }
                    if let Some(ref l) = input.re_label {
                        *label = l.clone();
                    }
                }
                Modifier::RowBy { value } => {
                    if let Ok(v) = val.parse::<u8>() {
                        *value = v.clamp(2, 8);
                    }
                }
                _ => {}
            }
        }
    }
    let sel = input.segment_id.as_deref().unwrap_or(&input.group_id);
    Html(
        templates::timeline::editor_content(&tl, &base_url, Some(sel), input.base.editor_state())
            .into_string(),
    )
}

/// `POST .../modifier-toggle` — toggle a value in a multi-select modifier (pause, drills).
pub(crate) async fn modifier_toggle(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<ModifierForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
    let mut tl = input.base.parse();
    if let Some(mods) = find_modifier_target(
        &mut tl,
        &input.group_id,
        input.segment_id.as_deref(),
        input.scope.as_deref(),
    ) {
        if let Some(m) = mods.iter_mut().find(|m| m.kind_id() == input.kind) {
            let val = input.value.as_deref().unwrap_or("");
            match m {
                Modifier::PauseAt { points, .. } => {
                    if let Ok(pp) = val.parse::<PausePoint>() {
                        if let Some(pos) = points.iter().position(|p| *p == pp) {
                            points.remove(pos);
                        } else {
                            points.push(pp);
                        }
                    }
                }
                Modifier::Drills { values } => {
                    if let Ok(hd) = val.parse::<HandDrill>() {
                        if let Some(pos) = values.iter().position(|d| *d == hd) {
                            values.remove(pos);
                        } else {
                            values.push(hd);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let sel = input.segment_id.as_deref().unwrap_or(&input.group_id);
    Html(
        templates::timeline::editor_content(&tl, &base_url, Some(sel), input.base.editor_state())
            .into_string(),
    )
}

/// `POST .../modifier-override` — copy an inherited modifier to the segment for local editing.
pub(crate) async fn modifier_override(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<ModifierForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
    let mut tl = input.base.parse();
    // Find the group modifier and clone it to the segment
    for item in &mut tl.items {
        if let TimelineItem::Group(g) = item {
            if g.id == input.group_id {
                if let Some(gm) = g.modifiers.iter().find(|m| m.kind_id() == input.kind) {
                    let cloned = gm.clone();
                    if let Some(sid) = &input.segment_id {
                        if let Some(s) = g.segments.iter_mut().find(|s| s.id == *sid) {
                            // Only add if not already overridden
                            if !s.modifiers.iter().any(|m| m.kind_id() == input.kind) {
                                s.modifiers.push(cloned);
                            }
                        }
                    }
                }
                break;
            }
        }
    }
    let sel = input.segment_id.as_deref().unwrap_or(&input.group_id);
    Html(
        templates::timeline::editor_content(&tl, &base_url, Some(sel), input.base.editor_state())
            .into_string(),
    )
}

/// `POST .../modifier-revert` — remove a segment-level override, restoring inheritance.
pub(crate) async fn modifier_revert(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<ModifierForm>,
) -> Html<String> {
    let base_url = practice_timeline_url(practice_id);
    let mut tl = input.base.parse();
    for item in &mut tl.items {
        if let TimelineItem::Group(g) = item {
            if g.id == input.group_id {
                if let Some(sid) = &input.segment_id {
                    if let Some(s) = g.segments.iter_mut().find(|s| s.id == *sid) {
                        s.modifiers.retain(|m| m.kind_id() != input.kind);
                    }
                }
                break;
            }
        }
    }
    let sel = input.segment_id.as_deref().unwrap_or(&input.group_id);
    Html(
        templates::timeline::editor_content(&tl, &base_url, Some(sel), input.base.editor_state())
            .into_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form<T: serde::de::DeserializeOwned>(qs: &str) -> T {
        serde_html_form::from_str(qs).unwrap()
    }

    // ── PatchBlockForm ──

    #[test]
    fn patch_block_form_parses_duration_unit() {
        let f: PatchBlockForm =
            form("patch_id=blk-1&duration_value=5&duration_unit=meters&note=test");
        assert_eq!(f.duration_unit, Some(DurationUnit::Meters));
        assert_eq!(f.duration_value, Some(5.0));
    }

    #[test]
    fn patch_block_form_missing_optional_fields() {
        let f: PatchBlockForm = form("patch_id=blk-1");
        assert_eq!(f.duration_unit, None);
        assert_eq!(f.duration_value, None);
        assert_eq!(f.note, None);
    }

    // ── PatchSegmentForm ──

    #[test]
    fn patch_segment_form_parses_base_fields() {
        let f: PatchSegmentForm = form(
            "group_id=g1&segment_id=s1\
             &duration_unit=strokes\
             &intensity=ut2\
             &seg_type=rest",
        );
        assert_eq!(f.duration_unit, Some(DurationUnit::Strokes));
        assert_eq!(f.intensity, Some(Intensity::Ut2));
        assert_eq!(f.seg_type, Some(SegmentType::Rest));
    }

    #[test]
    fn patch_segment_form_defaults_when_fields_absent() {
        let f: PatchSegmentForm = form("group_id=g1&segment_id=s1");
        assert_eq!(f.duration_unit, None);
        assert_eq!(f.intensity, None);
        assert_eq!(f.seg_type, None);
    }

    // ── GroupPatchForm ──

    #[test]
    fn group_patch_form_parses_group_type() {
        let f: GroupPatchForm = form("group_id=g1&group_type=piece");
        assert_eq!(f.group_type, Some(GroupType::Piece));
    }

    // ── GroupAddForm ──

    #[test]
    fn group_add_form_parses_seg_type() {
        let f: GroupAddForm = form("group_id=g1&seg_type=rest");
        assert_eq!(f.seg_type, SegmentType::Rest);
    }

    #[test]
    fn group_add_form_work_seg_type() {
        let f: GroupAddForm = form("group_id=g1&seg_type=work");
        assert_eq!(f.seg_type, SegmentType::Work);
    }
}
