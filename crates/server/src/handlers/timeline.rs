//! Timeline editor handlers.
//!
//! The timeline editor keeps its state in a hidden form field (`timeline`
//! containing JSON).  Each action mutates the timeline and returns the
//! re-rendered editor fragment via HTMX swap.

use axum::{extract::Path, response::Html, Extension, Form};
use lineup_db::{
    practice::{Practice, PracticeId},
    timeline::{
        self, Block, BlockType, Duration, DurationUnit, Group, GroupType, RotatePer, Rotation,
        Segment, SegmentType, Timeline, TimelineItem,
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
}

impl TimelineForm {
    fn parse(&self) -> Timeline {
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

fn make_id() -> String {
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

fn default_segment() -> Segment {
    Segment {
        id: make_id(),
        seg_type: SegmentType::Work,
        duration: Duration {
            value: 5.0,
            unit: DurationUnit::Min,
        },
        rate: Some([20, 20]),
        intensity: Some(timeline::Intensity::Paddle),
        partial: None,
        pause: vec![],
        pause_every: None,
        blade: None,
        drills: vec![],
        note: String::new(),
    }
}

/// `GET /history/{id}/timeline/edit` — open the timeline editor.
pub(crate) async fn open_editor(
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
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

    let html = templates::timeline::editor(&timeline, practice_id, None);
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
                templates::timeline::editor(&tl, practice_id, input.base.selected.as_deref())
                    .into_string(),
            )
        }
    };

    tl.insert_before_dock(vec![new_item]);
    Html(templates::timeline::editor(&tl, practice_id, Some(&select_id)).into_string())
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
    let mut tl = input.base.parse();
    tl.items
        .retain(|it| it.is_structural() || it.id() != input.delete_id);
    Html(templates::timeline::editor(&tl, practice_id, None).into_string())
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
    duration_unit: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

pub(crate) async fn patch_block(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<PatchBlockForm>,
) -> Html<String> {
    let mut tl = input.base.parse();

    for item in &mut tl.items {
        if let TimelineItem::Block(b) = item {
            if b.id == input.patch_id {
                if let Some(v) = input.duration_value {
                    b.duration.value = v.max(0.0);
                }
                if let Some(ref u) = input.duration_unit {
                    b.duration.unit = match u.as_str() {
                        "meters" => DurationUnit::Meters,
                        "strokes" => DurationUnit::Strokes,
                        _ => DurationUnit::Min,
                    };
                }
                if let Some(ref n) = input.note {
                    b.note = n.clone();
                }
                break;
            }
        }
    }

    Html(
        templates::timeline::editor_no_animate(&tl, practice_id, Some(&input.patch_id))
            .into_string(),
    )
}

/// `POST /history/{id}/timeline/patch-segment` — update fields on a segment inside a group.
#[derive(Debug, Deserialize)]
pub(crate) struct PatchSegmentForm {
    #[serde(flatten)]
    base: TimelineForm,
    group_id: String,
    segment_id: String,
    #[serde(default)]
    duration_value: Option<f64>,
    #[serde(default)]
    duration_unit: Option<String>,
    #[serde(default)]
    rate_low: Option<u8>,
    #[serde(default)]
    rate_high: Option<u8>,
    #[serde(default)]
    intensity: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    partial: Option<String>,
    #[serde(default)]
    blade: Option<String>,
    /// Comma-separated pause point values from hidden input.
    #[serde(default)]
    pause_points: Option<String>,
    #[serde(default)]
    pause_every: Option<String>,
    /// Comma-separated drill values from hidden input.
    #[serde(default)]
    drills: Option<String>,
    #[serde(default)]
    _range_toggle: Option<String>,
    #[serde(default)]
    seg_type: Option<String>,
}

pub(crate) async fn patch_segment(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<PatchSegmentForm>,
) -> Html<String> {
    let mut tl = input.base.parse();

    for item in &mut tl.items {
        if let TimelineItem::Group(g) = item {
            if g.id == input.group_id {
                for s in &mut g.segments {
                    if s.id == input.segment_id {
                        if let Some(v) = input.duration_value {
                            s.duration.value = v.max(0.0);
                        }
                        if let Some(ref u) = input.duration_unit {
                            s.duration.unit = match u.as_str() {
                                "meters" => DurationUnit::Meters,
                                "strokes" => DurationUnit::Strokes,
                                _ => DurationUnit::Min,
                            };
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
                        if let Some(ref int) = input.intensity {
                            s.intensity = match int.as_str() {
                                "paddle" => Some(timeline::Intensity::Paddle),
                                "ut2" => Some(timeline::Intensity::Ut2),
                                "ut1" => Some(timeline::Intensity::Ut1),
                                "at" => Some(timeline::Intensity::At),
                                "tr" => Some(timeline::Intensity::Tr),
                                "an" => Some(timeline::Intensity::An),
                                _ => s.intensity,
                            };
                        }
                        if let Some(ref n) = input.note {
                            s.note = n.clone();
                        }
                        if let Some(ref sl) = input.partial {
                            s.partial = parse_slide(sl);
                        }
                        if let Some(ref bl) = input.blade {
                            s.blade = match bl.as_str() {
                                "square" => Some(timeline::Blade::Square),
                                "partial-feather" => Some(timeline::Blade::PartialFeather),
                                "none" => None,
                                _ => Some(timeline::Blade::Feather),
                            };
                        }
                        if let Some(ref pp_csv) = input.pause_points {
                            s.pause = pp_csv
                                .split(',')
                                .filter(|s| !s.is_empty())
                                .filter_map(|p| parse_pause_point(p.trim()))
                                .collect();
                        }
                        if let Some(ref pe) = input.pause_every {
                            s.pause_every = pe.parse::<u32>().ok().filter(|&n| n > 0);
                        }
                        if let Some(ref dr_csv) = input.drills {
                            s.drills = dr_csv
                                .split(',')
                                .filter(|s| !s.is_empty())
                                .filter_map(|h| parse_hand_drill(h.trim()))
                                .collect();
                        }
                        if let Some(ref st) = input.seg_type {
                            s.seg_type = match st.as_str() {
                                "rest" => SegmentType::Rest,
                                "turn" => SegmentType::Turn,
                                _ => SegmentType::Work,
                            };
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
        templates::timeline::editor_no_animate(&tl, practice_id, Some(&input.segment_id))
            .into_string(),
    )
}

fn parse_slide(s: &str) -> Option<timeline::Slide> {
    match s {
        "full" => Some(timeline::Slide::Full),
        "arms-only" => Some(timeline::Slide::ArmsOnly),
        "arms-body" => Some(timeline::Slide::ArmsBody),
        "quarter" => Some(timeline::Slide::Quarter),
        "half" => Some(timeline::Slide::Half),
        "three-quarter" => Some(timeline::Slide::ThreeQuarter),
        "full-legs" => Some(timeline::Slide::FullLegs),
        "legs-body" => Some(timeline::Slide::LegsBody),
        _ => None,
    }
}

fn parse_pause_point(s: &str) -> Option<timeline::PausePoint> {
    match s {
        "release" => Some(timeline::PausePoint::Release),
        "arms-away" => Some(timeline::PausePoint::ArmsAway),
        "bodies-over" => Some(timeline::PausePoint::BodiesOver),
        "three-quarter" => Some(timeline::PausePoint::ThreeQuarter),
        "half" => Some(timeline::PausePoint::Half),
        "quarter" => Some(timeline::PausePoint::Quarter),
        "catch" => Some(timeline::PausePoint::Catch),
        _ => None,
    }
}

fn parse_hand_drill(s: &str) -> Option<timeline::HandDrill> {
    match s {
        "feet-out" => Some(timeline::HandDrill::FeetOut),
        "inside-arm" => Some(timeline::HandDrill::InsideArm),
        "outside-arm" => Some(timeline::HandDrill::OutsideArm),
        "inside-leg" => Some(timeline::HandDrill::InsideLeg),
        "outside-leg" => Some(timeline::HandDrill::OutsideLeg),
        "cut-the-cake" => Some(timeline::HandDrill::CutTheCake),
        "gunnel-taps" => Some(timeline::HandDrill::GunnelTaps),
        "wide-grip" => Some(timeline::HandDrill::WideGrip),
        "slap-catches" => Some(timeline::HandDrill::SlapCatches),
        _ => None,
    }
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
    let mut tl = input.base.parse();
    tl.target_minutes = input.new_target.clamp(20, 240);
    Html(
        templates::timeline::editor_no_animate(&tl, practice_id, input.base.selected.as_deref())
            .into_string(),
    )
}

/// `POST /history/{id}/timeline/save` — persist to DB.
pub(crate) async fn save_timeline(
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<TimelineForm>,
) -> Result<Html<String>, crate::handlers::ErrorResponse> {
    let tl = input.parse();
    let tl_for_db = tl.clone();
    tenant
        .db
        .with_conn(move |conn| Practice::update_timeline(conn, practice_id, Some(&tl_for_db)))
        .await
        .map_err(internal_error)?;
    Ok(Html(
        templates::timeline::summary(&tl, practice_id).into_string(),
    ))
}

/// `POST /history/{id}/timeline/close` — close without saving.
pub(crate) async fn close_editor(
    Extension(tenant): Extension<TenantContext>,
    Path(practice_id): Path<PracticeId>,
) -> Result<Html<String>, crate::handlers::ErrorResponse> {
    let practice = tenant
        .db
        .with_conn(move |conn| Practice::get(conn, practice_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error(diesel::result::Error::NotFound))?;
    let timeline = practice.timeline();
    Ok(Html(
        templates::timeline::summary(
            timeline.as_ref().unwrap_or(&Timeline::default_empty(90)),
            practice_id,
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
    Html(templates::timeline::editor(&tl, practice_id, Some(&input.drag_id)).into_string())
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
                templates::timeline::editor(&tl, practice_id, Some(&new_id)).into_string(),
            );
        }
    }
    Html(
        templates::timeline::editor(&tl, practice_id, input.base.selected.as_deref()).into_string(),
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
    group_type: Option<String>,
    #[serde(default)]
    prev_seg_type: Option<String>,
}

pub(crate) async fn group_patch(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<GroupPatchForm>,
) -> Html<String> {
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
                                    let u = match input.rotate_per_unit.as_deref() {
                                        Some("min") => DurationUnit::Min,
                                        Some("meters") => DurationUnit::Meters,
                                        _ => DurationUnit::Strokes,
                                    };
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
                if let Some(ref gt) = input.group_type {
                    g.group_type = match gt.as_str() {
                        "piece" => GroupType::Piece,
                        _ => GroupType::Warmup,
                    };
                }
                break;
            }
        }
    }
    // Use selected from form (may be a segment ID) or fall back to group.
    let sel = input.base.selected.as_deref().unwrap_or(&input.group_id);

    // Determine if the segment type changed (to decide animation).
    let new_seg_type = tl.items.iter().find_map(|it| {
        if let TimelineItem::Group(g) = it {
            g.segments.iter().find(|s| s.id == sel).map(|s| s.seg_type)
        } else {
            None
        }
    });
    let type_changed = match (&input.prev_seg_type, new_seg_type) {
        (Some(prev), Some(new_type)) => {
            let prev_type = match prev.as_str() {
                "work" => SegmentType::Work,
                "rest" => SegmentType::Rest,
                "turn" => SegmentType::Turn,
                _ => SegmentType::Work,
            };
            prev_type != new_type
        }
        (None, Some(_)) => true, // no previous = first selection, animate
        _ => false,
    };

    let render = if type_changed {
        templates::timeline::editor
    } else {
        templates::timeline::editor_no_animate
    };
    Html(render(&tl, practice_id, Some(sel)).into_string())
}

/// `POST /history/{id}/timeline/group-add` — add a segment to a group.
#[derive(Debug, Deserialize)]
pub(crate) struct GroupAddForm {
    #[serde(flatten)]
    base: TimelineForm,
    group_id: String,
    seg_type: String,
}

pub(crate) async fn group_add_segment(
    Path(practice_id): Path<PracticeId>,
    Form(input): Form<GroupAddForm>,
) -> Html<String> {
    let mut tl = input.base.parse();
    let st = match input.seg_type.as_str() {
        "rest" => SegmentType::Rest,
        "turn" => SegmentType::Turn,
        _ => SegmentType::Work,
    };
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
    Html(templates::timeline::editor(&tl, practice_id, Some(&new_id)).into_string())
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
    Html(templates::timeline::editor(&tl, practice_id, Some(&input.group_id)).into_string())
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
        return Html(templates::timeline::editor(&tl, practice_id, Some(&select_id)).into_string());
    }
    Html(
        templates::timeline::editor(&tl, practice_id, input.base.selected.as_deref()).into_string(),
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
    Html(templates::timeline::editor(&tl, practice_id, Some(&input.drag_id)).into_string())
}
