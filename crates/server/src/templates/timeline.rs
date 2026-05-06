//! Timeline editor + summary templates.

use lineup_db::{
    practice::PracticeId,
    timeline::{
        self, Blade, Block, BlockType, DurationUnit, Group, GroupType, HandDrill, Intensity,
        ItemDisplayType, PausePoint, RotatePer, Segment, SegmentType, Slide, Timeline,
        TimelineItem,
    },
};
use maud::{html, Markup};

use super::history::block_type_css;

// ── Summary (collapsed view) ─────────────────────────────────────────

pub(crate) fn summary(tl: &Timeline, practice_id: PracticeId) -> Markup {
    let lines = tl.summary_lines();
    let planned = tl.planned_minutes();
    let slack = tl.slack_minutes();

    html! {
        div id="timeline-section" class="rounded-lg pl-4 py-1" style="border-left: 3px solid var(--accent)" {
            div {
                div class="flex items-baseline justify-between gap-2 mb-1" {
                    span class="font-mono-stat text-[9.5px] tracking-[0.16em] uppercase font-semibold" style="color: var(--accent)" {
                        "Practice plan"
                    }
                    div class="flex items-center gap-3" {
                        span class="font-mono-stat text-[10px]" style="color: var(--muted)" {
                            (format!("{:.0}", planned)) " of " (tl.target_minutes) " min"
                        }
                        @if slack > 0.0 {
                            span class="font-mono-stat text-[10px]" style="color: var(--good)" {
                                "+" (format!("{:.0}", slack)) " slack"
                            }
                        } @else if slack < -1.0 {
                            span class="font-mono-stat text-[10px]" style="color: var(--bad)" {
                                (format!("{:.0}", slack)) " over"
                            }
                        }
                        button class="font-mono-stat text-[10.5px] hover:underline cursor-pointer px-1"
                               style="color: var(--muted); background: none; border: none"
                               hx-get={"/history/" (practice_id) "/timeline/edit"}
                               hx-target="#timeline-section"
                               hx-swap="outerHTML" {
                            "edit plan"
                        }
                    }
                }
                @if lines.is_empty() {
                    p class="text-sm italic m-0" style="color: var(--muted)" { "No plan yet." }
                } @else {
                    ol class="list-none m-0 p-0 space-y-1" {
                        @for line in &lines {
                            li {
                                @match &line.item_type {
                                    ItemDisplayType::Group(gt) => {
                                        div class="flex items-center gap-2" {
                                            span class="font-mono-stat text-[9px] px-1 py-px rounded border"
                                                 style=(group_type_css(*gt)) {
                                                (gt.label())
                                            }
                                            span class="font-serif-heading font-medium text-sm" style="color: var(--ink)" {
                                                (line.label)
                                            }
                                            span class="font-mono-stat text-[9px]" style="color: var(--muted)" {
                                                (line.duration_label)
                                            }
                                            @if let Some(ref rot) = line.rotation_label {
                                                span class="font-mono-stat text-[9px] italic" style="color: var(--muted)" {
                                                    " · " (rot)
                                                }
                                            }
                                        }
                                        ul class="list-none m-0 pl-5 mt-0.5 space-y-0.5" {
                                            @for seg in &line.children {
                                                li class="flex items-center gap-1.5 text-xs" {
                                                    span class="font-mono-stat text-[9px] px-1 py-px rounded border"
                                                         style=(seg_type_css(seg.seg_type)) {
                                                        (seg.seg_type.label())
                                                    }
                                                    span class="font-mono-stat" style="color: var(--ink-2)" { (seg.label) }
                                                    @if !seg.note.is_empty() {
                                                        span class="italic" style="color: var(--muted)" { "— " (seg.note) }
                                                    }
                                                }
                                            }
                                            @if let Some(ref instr) = line.repeat_instruction {
                                                li class="flex items-center gap-1.5 text-xs mt-0.5" {
                                                    span class="font-mono-stat text-[9px] px-1 py-px rounded border"
                                                         style="color: var(--bad); background: color-mix(in oklch, var(--bad) 10%, var(--paper)); border-color: color-mix(in oklch, var(--bad) 25%, var(--rule))" {
                                                        "Repeat"
                                                    }
                                                    span class="font-mono-stat" style="color: var(--ink-2)" { (instr) }
                                                }
                                            }
                                        }
                                    }
                                    ItemDisplayType::Block(bt) => {
                                        div class="flex items-center gap-1.5 text-xs" {
                                            span class="font-mono-stat text-[9px] px-1 py-px rounded border" style=(block_type_css(*bt)) {
                                                (bt.label())
                                            }
                                            span class="font-mono-stat" style="color: var(--ink-2)" { (line.duration_label) }
                                            @if !line.note.is_empty() {
                                                span class="italic" style="color: var(--muted)" { "— " (line.note) }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── CSS helpers ──────────────────────────────────────────────────────

fn group_type_css(gt: GroupType) -> &'static str {
    match gt {
        GroupType::Warmup => "color: var(--good); background: color-mix(in oklch, var(--good) 12%, var(--paper)); border-color: color-mix(in oklch, var(--good) 28%, var(--rule))",
        GroupType::Piece => "color: var(--accent); background: color-mix(in oklch, var(--accent) 12%, var(--paper)); border-color: color-mix(in oklch, var(--accent) 28%, var(--rule))",
    }
}

fn seg_type_css(st: SegmentType) -> &'static str {
    match st {
        SegmentType::Work => "color: var(--ink-2); background: var(--paper-2); border-color: var(--rule)",
        SegmentType::Rest => "color: var(--muted); background: var(--paper-2); border-color: var(--rule)",
        SegmentType::Turn => "color: var(--warn); background: color-mix(in oklch, var(--warn) 12%, var(--paper)); border-color: color-mix(in oklch, var(--warn) 28%, var(--rule))",
    }
}

fn strip_bg_block(bt: BlockType) -> &'static str {
    match bt {
        BlockType::Launch | BlockType::Dock => {
            "background: color-mix(in oklch, var(--cox) 18%, var(--paper))"
        }
        BlockType::Rest => "background: var(--paper-2)",
        BlockType::Turn => "background: color-mix(in oklch, var(--warn) 18%, var(--paper))",
    }
}

fn strip_bg_group(gt: GroupType) -> &'static str {
    match gt {
        GroupType::Warmup => "background: color-mix(in oklch, var(--good) 12%, var(--paper))",
        GroupType::Piece => "background: color-mix(in oklch, var(--accent) 12%, var(--paper))",
    }
}

fn strip_bg_seg(st: SegmentType, gt: GroupType) -> &'static str {
    match st {
        SegmentType::Work => match gt {
            GroupType::Warmup => "background: color-mix(in oklch, var(--good) 22%, var(--paper))",
            GroupType::Piece => "background: color-mix(in oklch, var(--accent) 22%, var(--paper))",
        },
        SegmentType::Rest => "background: var(--paper-3)",
        SegmentType::Turn => "background: color-mix(in oklch, var(--warn) 18%, var(--paper))",
    }
}

fn slide_value(s: Slide) -> &'static str {
    match s {
        Slide::Full => "full",
        Slide::ArmsOnly => "arms-only",
        Slide::ArmsBody => "arms-body",
        Slide::Quarter => "quarter",
        Slide::Half => "half",
        Slide::ThreeQuarter => "three-quarter",
        Slide::FullLegs => "full-legs",
        Slide::LegsBody => "legs-body",
    }
}

fn pause_value(p: PausePoint) -> &'static str {
    match p {
        PausePoint::Release => "release",
        PausePoint::ArmsAway => "arms-away",
        PausePoint::BodiesOver => "bodies-over",
        PausePoint::ThreeQuarter => "three-quarter",
        PausePoint::Half => "half",
        PausePoint::Quarter => "quarter",
        PausePoint::Catch => "catch",
    }
}

fn hand_drill_value(h: HandDrill) -> &'static str {
    match h {
        HandDrill::FeetOut => "feet-out",
        HandDrill::InsideArm => "inside-arm",
        HandDrill::OutsideArm => "outside-arm",
        HandDrill::InsideLeg => "inside-leg",
        HandDrill::OutsideLeg => "outside-leg",
        HandDrill::CutTheCake => "cut-the-cake",
        HandDrill::GunnelTaps => "gunnel-taps",
        HandDrill::WideGrip => "wide-grip",
        HandDrill::SlapCatches => "slap-catches",
    }
}

fn format_duration_short(minutes: f64) -> String {
    if minutes >= 1.0 {
        format!("{:.0}'", minutes)
    } else {
        format!("{:.1}'", minutes)
    }
}

fn block_tooltip(b: &Block, is_dock: bool, slack: f64) -> String {
    if is_dock {
        return if slack > 0.0 {
            format!("Dock — {:.0} min slack", slack)
        } else {
            format!("Dock — {:.0} min over target", -slack)
        };
    }
    let mut parts = vec![b.block_type.label().to_string(), b.duration.display()];
    if !b.note.is_empty() {
        parts.push(format!("— {}", b.note));
    }
    parts.join(" · ")
}

fn segment_tooltip(s: &Segment) -> String {
    let mut parts = vec![s.seg_type.label().to_string(), s.duration.display()];
    if let Some([lo, hi]) = s.rate {
        if lo == hi {
            parts.push(format!("r{lo}"));
        } else {
            parts.push(format!("r{lo}-{hi}"));
        }
    }
    if let Some(int) = s.intensity {
        parts.push(format!("@ {}", int.full_name()));
    }
    if let Some(sl) = s.partial {
        if sl != Slide::Full {
            parts.push(sl.label().to_string());
        }
    }
    if !s.pause.is_empty() {
        let labels: Vec<&str> = s.pause.iter().map(|p| p.label()).collect();
        parts.push(format!("pause @ {}", labels.join(" + ")));
    }
    match s.blade {
        Some(Blade::Square) => parts.push("on square".to_string()),
        Some(Blade::PartialFeather) => parts.push("partial feather".to_string()),
        _ => {}
    }
    for hd in &s.drills {
        parts.push(hd.label().to_string());
    }
    if !s.note.is_empty() {
        parts.push(format!("— {}", s.note));
    }
    parts.join(" · ")
}

fn group_tooltip(g: &Group) -> String {
    let mut parts = vec![
        format!(
            "{}: {}",
            g.group_type.label(),
            if g.name.is_empty() {
                "(unnamed)"
            } else {
                &g.name
            }
        ),
        format!("{} segments", g.segments.len()),
        format!("{:.0} min", g.approx_minutes()),
    ];
    if g.rotation.is_active() {
        parts.push(g.rotation.label());
    }
    if !g.note.is_empty() {
        parts.push(format!("— {}", g.note));
    }
    parts.join(" · ")
}

// ── Editor (expanded view) ───────────────────────────────────────────

pub(crate) fn editor(tl: &Timeline, practice_id: PracticeId, selected_id: Option<&str>) -> Markup {
    editor_inner(tl, practice_id, selected_id, true)
}

pub(crate) fn editor_no_animate(
    tl: &Timeline,
    practice_id: PracticeId,
    selected_id: Option<&str>,
) -> Markup {
    editor_inner(tl, practice_id, selected_id, false)
}

fn editor_inner(
    tl: &Timeline,
    practice_id: PracticeId,
    selected_id: Option<&str>,
    animate: bool,
) -> Markup {
    let tl_json = serde_json::to_string(tl).unwrap_or_else(|_| "{}".to_string());
    let planned = tl.planned_minutes();
    let slack = tl.slack_minutes();
    let slack_state = if slack > 5.0 {
        "ok"
    } else if slack >= -1.0 {
        "tight"
    } else {
        "over"
    };

    // Find selected item.
    enum Sel<'a> {
        Block(&'a Block),
        Group(&'a Group),
        None,
    }
    let selected = match selected_id {
        Some(sid) => {
            let mut found = Sel::None;
            for item in &tl.items {
                match item {
                    TimelineItem::Block(b) if b.id == sid => {
                        found = Sel::Block(b);
                        break;
                    }
                    TimelineItem::Group(g) if g.id == sid => {
                        found = Sel::Group(g);
                        break;
                    }
                    // Also check if a segment inside a group is selected — select the group.
                    TimelineItem::Group(g) if g.segments.iter().any(|s| s.id == sid) => {
                        found = Sel::Group(g);
                        break;
                    }
                    _ => {}
                }
            }
            found
        }
        None => Sel::None,
    };

    let base_url = format!("/history/{practice_id}/timeline");

    html! {
        div id="timeline-section" class="rounded-lg pl-4 py-1 mb-4" style="border-left: 3px solid var(--accent)" {
            // Header
            div class="flex items-center justify-between mb-3" {
                div class="flex items-baseline gap-2" {
                    span class="font-mono-stat text-[9.5px] tracking-[0.16em] uppercase font-semibold" style="color: var(--accent)" { "Practice plan" }
                    span class="font-mono-stat text-[9px]" style="color: var(--muted)" { " · click a block to edit" }
                }
                div class="flex items-center gap-3" {
                    // Duration meter
                    div class="flex items-center gap-1" {
                        span class="font-mono-stat text-sm font-medium" style="color: var(--ink)" { (format!("{:.0}", planned)) }
                        span class="font-mono-stat text-[10px]" style="color: var(--muted)" { "/" }
                        form class="inline" hx-post={(base_url) "/target"} hx-target="#timeline-section" hx-swap="outerHTML" {
                            input type="hidden" name="timeline" value=(tl_json);
                            input type="hidden" name="selected" value=(selected_id.unwrap_or(""));
                            input type="number" name="new_target" min="20" max="240" value=(tl.target_minutes)
                                  class="font-mono-stat text-sm font-medium w-10 text-center border-b"
                                  style="color: var(--ink); background: transparent; border-color: var(--rule); border-width: 0 0 1px 0; padding: 0; outline: none"
                                  onchange="this.form.requestSubmit()";
                            span class="font-mono-stat text-[10px]" style="color: var(--muted)" { " min" }
                        }
                    }
                    @match slack_state {
                        "ok" => { span class="font-mono-stat text-[10px]" style="color: var(--good)" { "+" (format!("{:.0}", slack)) " slack" } }
                        "tight" => { span class="font-mono-stat text-[10px]" style="color: var(--warn)" { "tight" } }
                        _ => { span class="font-mono-stat text-[10px]" style="color: var(--bad)" { (format!("{:.0}", slack)) " over" } }
                    }
                    form class="inline" hx-post={(base_url) "/save"} hx-target="#timeline-section" hx-swap="outerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        button type="submit" class="btn-warm-ink text-xs py-1.5 px-3" { "Save" }
                    }
                    button class="btn-warm-ghost text-xs py-1.5 px-3"
                           hx-post={(base_url) "/close"} hx-target="#timeline-section" hx-swap="outerHTML" { "Cancel" }
                }
            }

            // Palette
            div class="flex flex-wrap gap-1.5 items-center mb-3 pb-3" style="border-bottom: 1px solid var(--rule-2)" {
                span class="font-mono-stat text-[9px] tracking-wider uppercase self-center mr-1" style="color: var(--muted)" { "Add" }
                // Warmup / Piece create groups
                @for (add_type, label, css) in &[
                    ("warmup", "Warmup", group_type_css(GroupType::Warmup)),
                    ("piece", "Piece", group_type_css(GroupType::Piece)),
                ] {
                    form class="inline" hx-post={(base_url) "/add"} hx-target="#timeline-section" hx-swap="outerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        input type="hidden" name="add_type" value=(add_type);
                        button type="submit" class="font-mono-stat text-[10px] px-2 py-1 rounded border cursor-pointer hover:opacity-80" style=(css) { (label) }
                    }
                }
                // Bare blocks
                @for bt in BlockType::USER_ADDABLE {
                    form class="inline" hx-post={(base_url) "/add"} hx-target="#timeline-section" hx-swap="outerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        input type="hidden" name="add_type" value=(bt.label().to_lowercase());
                        button type="submit" class="font-mono-stat text-[10px] px-2 py-1 rounded border cursor-pointer hover:opacity-80" style=(block_type_css(*bt)) { (bt.label()) }
                    }
                }
                span style="width: 1px; height: 16px; background: var(--rule); margin: 0 2px" {}
                @for tmpl in &timeline::built_in_templates() {
                    form class="inline" hx-post={(base_url) "/template"} hx-target="#timeline-section" hx-swap="outerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        input type="hidden" name="template_id" value=(tmpl.id);
                        button type="submit" class="font-mono-stat text-[9px] px-2 py-1 rounded border cursor-pointer hover:opacity-80"
                               style="color: var(--ink-2); border-color: var(--rule); background: var(--paper)" title=(tmpl.description) { (tmpl.name) }
                    }
                }
            }

            // Hidden reorder forms (populated by drag JS, submitted via HTMX)
            form id="tl-reorder-form" class="hidden"
                 hx-post={(base_url) "/reorder"}
                 hx-target="#timeline-section"
                 hx-swap="outerHTML" {
                input type="hidden" name="timeline" value=(tl_json);
                input type="hidden" name="drag_id" value="";
                input type="hidden" name="drop_before_id" value="";
            }
            form id="tl-seg-reorder-form" class="hidden"
                 hx-post={(base_url) "/group-reorder"}
                 hx-target="#timeline-section"
                 hx-swap="outerHTML" {
                input type="hidden" name="timeline" value=(tl_json);
                input type="hidden" name="group_id" value=(selected_id.unwrap_or(""));
                input type="hidden" name="drag_id" value="";
                input type="hidden" name="drop_before_id" value="";
            }

            // Timeline strip
            (timeline_strip(tl, &base_url, &tl_json, selected_id))

            // Editor for selected item
            @match &selected {
                Sel::Block(block) => { (bare_block_editor(block, &base_url, &tl_json, animate)) }
                Sel::Group(group) => { (group_editor(group, &base_url, &tl_json, selected_id, animate)) }
                Sel::None => {}
            }

            // Drag-and-drop reorder JS
            (maud::PreEscaped(DRAG_REORDER_JS))
        }
    }
}

const DRAG_REORDER_JS: &str = r#"<script>
(function(){
  var dragId = null;
  var dragZone = null;
  var dropSide = null; // 'before' or 'after'
  var allClasses = ['tl-drop-before','tl-drop-after','tl-drop-above','tl-drop-below'];

  function clearDrop() {
    allClasses.forEach(function(c){
      document.querySelectorAll('.'+c).forEach(function(d){ d.classList.remove(c); });
    });
    dropSide = null;
  }

  function findTarget(el) {
    while (el) {
      if (el.dataset && el.dataset.dropId) return el;
      el = el.parentElement;
    }
    return null;
  }

  // Which half of the element is the cursor in?
  function getSide(el, e, zone) {
    var rect = el.getBoundingClientRect();
    if (zone === 'seglist') {
      return (e.clientY < rect.top + rect.height / 2) ? 'before' : 'after';
    }
    return (e.clientX < rect.left + rect.width / 2) ? 'before' : 'after';
  }

  function sideClass(side, zone) {
    if (zone === 'seglist') return side === 'before' ? 'tl-drop-above' : 'tl-drop-below';
    return side === 'before' ? 'tl-drop-before' : 'tl-drop-after';
  }

  document.querySelectorAll('[data-drag-id]').forEach(function(el){
    el.addEventListener('dragstart', function(e){
      dragId = el.dataset.dragId;
      dragZone = el.dataset.dragZone || 'strip';
      setTimeout(function(){
        el.classList.add('tl-dragging');
        el.style.pointerEvents = 'none';
      }, 0);
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', dragId);
    });
    el.addEventListener('dragend', function(){
      dragId = null;
      el.classList.remove('tl-dragging');
      el.style.pointerEvents = '';
      clearDrop();
    });
  });

  var section = document.getElementById('timeline-section');
  if (!section) return;

  section.addEventListener('dragover', function(e){
    if (!dragId) return;
    var target = findTarget(e.target);
    if (!target || target.dataset.dropId === dragId) { clearDrop(); return; }
    if (target.dataset.dragZone && target.dataset.dragZone !== dragZone) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    var side = getSide(target, e, dragZone);
    clearDrop();
    target.classList.add(sideClass(side, dragZone));
    dropSide = side;
  });

  section.addEventListener('drop', function(e){
    if (!dragId) return;
    var target = findTarget(e.target);
    if (!target || target.dataset.dropId === dragId) { clearDrop(); return; }
    e.preventDefault();
    var side = getSide(target, e, dragZone);
    var targetId = target.dataset.dropId;

    // For 'after', we need the next sibling's ID as drop_before_id.
    // If there's no next sibling, use a sentinel to append at end.
    if (side === 'after') {
      var siblings = target.parentElement.querySelectorAll('[data-drop-id]');
      var arr = Array.from(siblings).filter(function(s){ return s.dataset.dragZone === dragZone; });
      var idx = arr.indexOf(target);
      if (idx >= 0 && idx + 1 < arr.length) {
        targetId = arr[idx + 1].dataset.dropId;
      } else {
        targetId = '__end__';
      }
    }

    var form = (dragZone === 'seglist')
      ? document.getElementById('tl-seg-reorder-form')
      : document.getElementById('tl-reorder-form');
    if (!form) { clearDrop(); return; }
    form.querySelector('[name=drag_id]').value = dragId;
    form.querySelector('[name=drop_before_id]').value = targetId;
    // For segment reorder, resolve group_id from the list container.
    if (dragZone === 'seglist') {
      var listEl = target.closest('[data-seg-group-id]');
      if (listEl) form.querySelector('[name=group_id]').value = listEl.dataset.segGroupId;
    }
    clearDrop();
    htmx.trigger(form, 'submit');
  });
})();
</script>"#;

// ── Timeline strip ───────────────────────────────────────────────────

fn timeline_strip(
    tl: &Timeline,
    base_url: &str,
    tl_json: &str,
    selected_id: Option<&str>,
) -> Markup {
    let total_min: f64 = tl
        .items
        .iter()
        .map(|it| match it {
            TimelineItem::Block(b) if b.block_type == BlockType::Dock => {
                tl.slack_minutes().max(0.0)
            }
            _ => it.approx_minutes(),
        })
        .sum::<f64>()
        .max(1.0);

    html! {
        div class="flex gap-px mb-3 rounded overflow-hidden" style="height: 45px; background: var(--paper-2)" {
            @for item in &tl.items {
                @let id = item.id();
                @let is_selected = selected_id == Some(id) || match item {
                    TimelineItem::Group(g) => selected_id.is_some_and(|sid| g.segments.iter().any(|s| s.id == sid)),
                    _ => false,
                };
                @let border = if is_selected { "outline: 1.5px solid var(--ink-3); outline-offset: -1.5px; z-index: 1" } else { "" };

                @match item {
                    TimelineItem::Block(b) => {
                        @let is_dock = b.block_type == BlockType::Dock;
                        @let minutes = if is_dock { tl.slack_minutes().max(0.0) } else { b.approx_minutes() };
                        @let pct = (minutes / total_min * 100.0).max(2.0);
                        @let bg = if is_dock {
                            "background: repeating-linear-gradient(135deg, color-mix(in oklch, var(--cox) 8%, var(--paper)) 0px, color-mix(in oklch, var(--cox) 8%, var(--paper)) 4px, color-mix(in oklch, var(--cox) 14%, var(--paper)) 4px, color-mix(in oklch, var(--cox) 14%, var(--paper)) 8px)"
                        } else { strip_bg_block(b.block_type) };
                        @let tooltip = block_tooltip(b, is_dock, tl.slack_minutes());
                        @let is_structural = b.block_type.is_structural();
                        form class="inline" style={"flex: " (format!("{:.2}", pct)) "; min-width: 0; " (bg) "; " (border)}
                             title=(tooltip) hx-post={(base_url) "/patch-block"} hx-target="#timeline-section" hx-swap="outerHTML"
                             draggable={@if !is_structural { "true" } @else { "false" }}
                             data-drag-id=[(!is_structural).then_some(id)]
                             data-drop-id=[(!is_structural).then_some(id)]
                             data-drag-zone="strip" {
                            input type="hidden" name="timeline" value=(tl_json);
                            input type="hidden" name="patch_id" value=(id);
                            input type="hidden" name="selected" value=(id);
                            button type="submit" class="w-full h-full flex flex-col items-center cursor-pointer overflow-hidden px-1 pt-1"
                                   style="background: none; border: none; font: inherit" {
                                span class="font-mono-stat text-[8px] uppercase tracking-wider truncate w-full text-center" style="opacity: 0.7; pointer-events: none" { (b.block_type.label()) }
                                @if minutes > 0.0 {
                                    span class="font-mono-stat text-[9px] truncate w-full text-center" style="pointer-events: none" {
                                        @if is_dock {
                                            @if tl.slack_minutes() > 0.0 { "+" (format!("{:.0}", tl.slack_minutes())) "m" }
                                            @else { "0m" }
                                        } @else { (format_duration_short(minutes)) }
                                    }
                                }
                            }
                        }
                    }
                    TimelineItem::Group(g) => {
                        @let minutes = g.approx_minutes().max(0.01);
                        @let pct = (minutes / total_min * 100.0).max(2.0);
                        @let bg = strip_bg_group(g.group_type);
                        // Container: holds the group name label + individual segment buttons.
                        div style={"flex: " (format!("{:.2}", pct)) "; min-width: 0; " (bg) "; " (border)
                                    "; display: flex; flex-direction: column; border-radius: 2px; overflow: hidden"}
                            draggable="true"
                            data-drag-id=(id)
                            data-drop-id=(id)
                            data-drag-zone="strip" {
                            // Group name label (clickable — selects the group itself)
                            @if !g.name.is_empty() {
                                form class="block" style="line-height: 0"
                                     hx-post={(base_url) "/group-patch"} hx-target="#timeline-section" hx-swap="outerHTML" {
                                    input type="hidden" name="timeline" value=(tl_json);
                                    input type="hidden" name="group_id" value=(id);
                                    input type="hidden" name="selected" value=(g.segments.first().map(|s| s.id.as_str()).unwrap_or(id));
                                    button type="submit" class="w-full cursor-pointer truncate"
                                           style="font-size: 8px; font-family: ui-monospace, 'Cascadia Code', 'SF Mono', Menlo, monospace; letter-spacing: 0.08em; text-transform: uppercase; color: var(--ink-3); line-height: 1.4; background: none; border: none; padding: 4px 2px 0"
                                           title=(group_tooltip(g)) {
                                        (g.name)
                                    }
                                }
                            }
                            // Segment bars — repeated for each rotation.
                            // Selecting a segment highlights all its repetition instances.
                            @let reps = g.strip_repetitions();
                            @let seg_count = g.segments.len();
                            @let total_bars = seg_count * reps;
                            div class="flex gap-px flex-1" style="min-height: 0" {
                                @for bar_idx in 0..total_bars {
                                    @let s = &g.segments[bar_idx % seg_count];
                                    @let sm = s.approx_minutes();
                                    @let spct = sm.max(0.1);
                                    @let seg_selected = selected_id == Some(s.id.as_str());
                                    @let seg_border = if seg_selected { "outline: 1.5px solid var(--ink-3); outline-offset: -1.5px; z-index: 1" } else { "" };
                                    // Subtle separator between rotation boundaries
                                    @let rotation_gap = if bar_idx > 0 && bar_idx % seg_count == 0 { "margin-left: 2px" } else { "" };
                                    @let seg_tip = segment_tooltip(s);
                                    form class="inline" style={"flex: " (format!("{:.1}", spct)) "; min-width: 0; " (strip_bg_seg(s.seg_type, g.group_type)) "; border-radius: 1px; " (seg_border) "; " (rotation_gap)}
                                         title=(seg_tip) hx-post={(base_url) "/group-patch"} hx-target="#timeline-section" hx-swap="outerHTML" {
                                        input type="hidden" name="timeline" value=(tl_json);
                                        input type="hidden" name="group_id" value=(id);
                                        input type="hidden" name="selected" value=(s.id);
                                        button type="submit" class="w-full h-full cursor-pointer"
                                               style="background: none; border: none; font: inherit; padding: 0" {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Bare block editor (Launch/Rest/Turn/Dock) ────────────────────────

fn bare_block_editor(block: &Block, base_url: &str, tl_json: &str, animate: bool) -> Markup {
    let is_structural = block.block_type.is_structural();
    html! {
        div class={"mt-3 pt-3" @if animate { " tl-animate-in" }} style="border-top: 1px solid var(--rule-2)" {
            div class="flex items-center justify-between mb-3" {
                div class="flex items-center gap-2" {
                    span class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border font-semibold" style=(block_type_css(block.block_type)) { (block.block_type.label()) }
                    @if is_structural {
                        span class="font-mono-stat text-[9px] italic" style="color: var(--muted)" {
                            @if block.block_type == BlockType::Launch { "Fixed start" } @else { "Fixed end — auto-sizes to slack" }
                        }
                    }
                }
                @if !is_structural {
                    div class="flex items-center gap-1" {
                        form class="inline" hx-post={(base_url) "/duplicate"} hx-target="#timeline-section" hx-swap="outerHTML" {
                            input type="hidden" name="timeline" value=(tl_json);
                            input type="hidden" name="dup_id" value=(block.id);
                            button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer" style="color: var(--ink-2); border-color: var(--rule); background: var(--paper)" { "Duplicate" }
                        }
                        form class="inline" hx-post={(base_url) "/delete"} hx-target="#timeline-section" hx-swap="outerHTML" {
                            input type="hidden" name="timeline" value=(tl_json);
                            input type="hidden" name="delete_id" value=(block.id);
                            button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer" style="color: var(--bad); border-color: color-mix(in oklch, var(--bad) 30%, var(--rule)); background: var(--paper)" { "Delete" }
                        }
                    }
                }
            }
            @if block.block_type != BlockType::Dock {
                form hx-post={(base_url) "/patch-block"} hx-target="#timeline-section" hx-swap="outerHTML" hx-trigger="change" {
                    input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="patch_id" value=(block.id);
                    input type="hidden" name="selected" value=(block.id);
                    div class="flex flex-wrap gap-4 items-start" {
                        div {
                            label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Duration" }
                            div class="flex items-center gap-1" {
                                input type="number" name="duration_value" min="0" step="0.5" value=(block.duration.value) class="input-warm text-sm w-16 py-1";
                                select name="duration_unit" class="input-warm text-sm py-1" {
                                    option value="min" selected[block.duration.unit == timeline::DurationUnit::Min] { "min" }
                                    option value="meters" selected[block.duration.unit == timeline::DurationUnit::Meters] { "meters" }
                                    option value="strokes" selected[block.duration.unit == timeline::DurationUnit::Strokes] { "strokes" }
                                }
                            }
                        }
                        div {
                            label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Notes" }
                            textarea name="note" rows="1" placeholder="e.g. spin at the dam" class="input-warm text-sm w-full resize-y" { (block.note) }
                        }
                    }
                }
            }
        }
    }
}

// ── Group editor (Warmup/Piece) ──────────────────────────────────────

fn group_editor(
    group: &Group,
    base_url: &str,
    tl_json: &str,
    selected_id: Option<&str>,
    animate: bool,
) -> Markup {
    // Which segment is selected for drill editing?
    let selected_seg = selected_id.and_then(|sid| group.segments.iter().find(|s| s.id == sid));
    let cur_seg_type = selected_seg.map(|s| s.seg_type);

    html! {
        div class="mt-3 pt-3" style="border-top: 1px solid var(--rule-2)" {
            // Header
            div class="flex items-center justify-between mb-3" {
                div class="flex items-center gap-2" {
                    span class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border font-semibold" style=(group_type_css(group.group_type)) { (group.group_type.label()) }
                    span class="font-mono-stat text-[9px]" style="color: var(--muted)" {
                        (group.segments.len()) " segment" @if group.segments.len() != 1 { "s" }
                        " · " (format!("{:.0}", group.approx_minutes())) " min"
                    }
                }
                div class="flex items-center gap-1" {
                    // Type toggle
                    @let other_type = if group.group_type == GroupType::Warmup { "piece" } else { "warmup" };
                    @let other_label = if group.group_type == GroupType::Warmup { "→ Piece" } else { "→ Warmup" };
                    form class="inline" hx-post={(base_url) "/group-patch"} hx-target="#timeline-section" hx-swap="outerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        input type="hidden" name="group_id" value=(group.id);
                        input type="hidden" name="selected" value=(group.id);
                        input type="hidden" name="group_type" value=(other_type);
                        button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer" style="color: var(--ink-2); border-color: var(--rule); background: var(--paper)" { (other_label) }
                    }
                    form class="inline" hx-post={(base_url) "/duplicate"} hx-target="#timeline-section" hx-swap="outerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        input type="hidden" name="dup_id" value=(group.id);
                        button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer" style="color: var(--ink-2); border-color: var(--rule); background: var(--paper)" { "Duplicate" }
                    }
                    form class="inline" hx-post={(base_url) "/delete"} hx-target="#timeline-section" hx-swap="outerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        input type="hidden" name="delete_id" value=(group.id);
                        button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer" style="color: var(--bad); border-color: color-mix(in oklch, var(--bad) 30%, var(--rule)); background: var(--paper)" { "Delete" }
                    }
                }
            }

            // Group fields
            form hx-post={(base_url) "/group-patch"} hx-target="#timeline-section" hx-swap="outerHTML" hx-trigger="change" {
                input type="hidden" name="timeline" value=(tl_json);
                input type="hidden" name="group_id" value=(group.id);
                input type="hidden" name="selected" value=(group.id);
                div class="flex flex-wrap gap-4 items-start mb-3" {
                    div class="flex-1 min-w-[150px]" {
                        label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Name" }
                        input type="text" name="group_name" value=(group.name) placeholder="e.g. Pick drill" class="input-warm text-sm w-full py-1";
                    }
                    div {
                        label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Repeat" }
                        div class="flex items-center gap-1" {
                            input type="number" name="repeat" min="1" max="20"
                                  value=(group.repeat.unwrap_or(1))
                                  class="input-warm text-xs w-14 py-0.5 text-center";
                            span class="font-mono-stat text-[9px]" style="color: var(--muted)" { "×" }
                        }
                    }
                    div {
                        label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Rotation" }
                        div class="flex flex-wrap gap-2 items-center" {
                            div class="flex items-center gap-1" {
                                span class="font-mono-stat text-[9px]" style="color: var(--muted)" { "row by" }
                                select name="row_by" class="input-warm text-xs py-0.5" {
                                    option value="all" selected[group.rotation.row_by.is_none()] { "all" }
                                    @for n in &[6_u8, 4, 2] {
                                        option value=(n) selected[group.rotation.row_by == Some(*n)] { (n) }
                                    }
                                }
                            }
                            @if group.rotation.row_by.is_some() {
                                div class="flex items-center gap-1" {
                                    span class="font-mono-stat text-[9px]" style="color: var(--muted)" { "rotate by" }
                                    select name="rotate_by" class="input-warm text-xs py-0.5" {
                                        @for n in &[4_u8, 2, 1] {
                                            option value=(n) selected[group.rotation.rotate_by == Some(*n)] { (n) }
                                        }
                                    }
                                }
                                div class="flex items-center gap-1" {
                                    @let is_per_seg = group.rotation.rotate_per == RotatePer::Segment;
                                    @let is_per_group = group.rotation.rotate_per == RotatePer::Group;
                                    @let is_per_every = !is_per_seg && !is_per_group && group.rotation.rotate_per != RotatePer::None;
                                    select name="rotate_per" class="input-warm text-xs py-0.5" {
                                        option value="segment" selected[is_per_seg] { "per segment" }
                                        option value="group" selected[is_per_group] { "per group" }
                                        option value="every" selected[is_per_every] { "every" }
                                    }
                                    @if is_per_every {
                                        @let (ev_val, ev_unit) = match &group.rotation.rotate_per {
                                            RotatePer::Every { value, unit } => (*value, *unit),
                                            _ => (10.0, DurationUnit::Strokes),
                                        };
                                        input type="number" name="rotate_per_value" min="1" max="999"
                                              value=(ev_val)
                                              class="input-warm text-xs w-14 py-0.5 text-center";
                                        select name="rotate_per_unit" class="input-warm text-xs py-0.5" {
                                            option value="strokes" selected[ev_unit == DurationUnit::Strokes] { "strokes" }
                                            option value="min" selected[ev_unit == DurationUnit::Min] { "min" }
                                        }
                                    }
                                    // Rotations count (per-group only — per-segment
                                    // rotates after each segment automatically)
                                    @if is_per_group {
                                        @let default_rots = group.rotation.rotate_by.map(|rb| (8 / rb).max(1)).unwrap_or(2);
                                        div class="flex items-center gap-1" {
                                            input type="number" name="rotations" min="1" max="20"
                                                  value=(group.rotation.rotations.unwrap_or(default_rots))
                                                  class="input-warm text-xs w-14 py-0.5 text-center";
                                            span class="font-mono-stat text-[9px]" style="color: var(--muted)" { "rotations" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Segment list
            div class="space-y-1" data-seg-group-id=(group.id) {
                @for seg in &group.segments {
                    @let is_sel = selected_id == Some(seg.id.as_str());
                    div class={"flex items-center gap-1 px-2 py-1.5 rounded cursor-pointer" @if is_sel { " ring-1 ring-ink" }}
                         style={"background: var(--paper-2)" @if is_sel { "; background: var(--paper)" }}
                         draggable="true"
                         data-drag-id=(seg.id)
                         data-drop-id=(seg.id)
                         data-drag-zone="seglist" {
                        // Drag handle (6-dot grip)
                        span class="flex-shrink-0 cursor-grab"
                             style="display: grid; grid-template-columns: 4px 4px; gap: 2px; width: 14px; padding: 2px 2px; user-select: none"
                             title="Drag to reorder" {
                            @for _ in 0..6 {
                                span style="width: 3px; height: 3px; border-radius: 50%; background: var(--rule)" {}
                            }
                        }
                        // Click to select this segment for editing
                        form class="flex-1 flex items-center gap-2 min-w-0"
                             hx-post={(base_url) "/group-patch"} hx-target="#timeline-section" hx-swap="outerHTML" {
                            input type="hidden" name="timeline" value=(tl_json);
                            input type="hidden" name="group_id" value=(group.id);
                            input type="hidden" name="selected" value=(seg.id);
                            @if let Some(cst) = cur_seg_type {
                                input type="hidden" name="prev_seg_type" value=(format!("{:?}", cst).to_lowercase());
                            }
                            button type="submit" class="flex items-center gap-2 flex-1 min-w-0 text-left" style="background: none; border: none; font: inherit; cursor: pointer; padding: 0" {
                                span class="font-mono-stat text-[9px] px-1 py-px rounded border" style=(seg_type_css(seg.seg_type)) { (seg.seg_type.label()) }
                                span class="font-mono-stat text-xs flex-1 min-w-0 truncate" style="color: var(--ink-2)" {
                                    (seg.duration.display())
                                    @if let Some([lo, hi]) = seg.rate {
                                        " r" (lo) @if lo != hi { "-" (hi) }
                                    }
                                    @if let Some(int) = seg.intensity { " @" (int.label()) }
                                    @if let Some(sl) = seg.partial { @if sl != Slide::Full { " " (sl.label()) } }
                                }
                            }
                        }
                        // Delete segment (only if >1 segments remain)
                        @if group.segments.len() > 1 {
                            form class="inline" hx-post={(base_url) "/group-delete"} hx-target="#timeline-section" hx-swap="outerHTML" {
                                input type="hidden" name="timeline" value=(tl_json);
                                input type="hidden" name="group_id" value=(group.id);
                                input type="hidden" name="segment_id" value=(seg.id);
                                button type="submit" class="font-mono-stat text-[9px] px-1 py-px rounded cursor-pointer" style="color: var(--bad); background: none; border: none" title="Remove" { "\u{00d7}" }
                            }
                        }
                    }
                }
            }
            // Add segment buttons
            div class="flex gap-1 mt-1" {
                @for (st, label) in &[("work", "+ Segment"), ("rest", "+ Rest"), ("turn", "+ Turn")] {
                    form class="inline" hx-post={(base_url) "/group-add"} hx-target="#timeline-section" hx-swap="outerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        input type="hidden" name="group_id" value=(group.id);
                        input type="hidden" name="seg_type" value=(st);
                        button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer hover:opacity-80"
                               style="color: var(--ink-2); border-color: var(--rule); background: var(--paper)" { (label) }
                    }
                }
            }

            // Selected segment detail editor
            @if let Some(seg) = selected_seg {
                (segment_editor(seg, group, base_url, tl_json, animate))
            }
        }
    }
}

// ── Segment editor (within a group) ──────────────────────────────────

fn segment_editor(
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
    animate: bool,
) -> Markup {
    let is_work = seg.seg_type.is_work();
    html! {
        div class={"mt-3 pt-3" @if animate { " tl-animate-in" }} style="border-top: 1px dashed var(--rule-2)" {
            form hx-post={(base_url) "/patch-segment"} hx-target="#timeline-section" hx-swap="outerHTML" hx-trigger="change" {
                input type="hidden" name="timeline" value=(tl_json);
                input type="hidden" name="group_id" value=(group.id);
                input type="hidden" name="segment_id" value=(seg.id);
                input type="hidden" name="selected" value=(seg.id);

                div class="space-y-3" {
                    // Row 1: Duration + Intensity
                    div class="flex flex-wrap gap-4 items-start" {
                        div {
                            label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Duration" }
                            div class="flex items-center gap-1" {
                                input type="number" name="duration_value" min="0" step="0.5" value=(seg.duration.value) class="input-warm text-sm w-16 py-1";
                                select name="duration_unit" class="input-warm text-sm py-1" {
                                    option value="min" selected[seg.duration.unit == timeline::DurationUnit::Min] { "min" }
                                    option value="meters" selected[seg.duration.unit == timeline::DurationUnit::Meters] { "meters" }
                                    option value="strokes" selected[seg.duration.unit == timeline::DurationUnit::Strokes] { "strokes" }
                                }
                            }
                        }
                        @if is_work {
                            div {
                                label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Intensity" }
                                div class="flex flex-wrap gap-1" {
                                    @for int in Intensity::ALL {
                                        @let is_active = seg.intensity == Some(*int);
                                        label class="cursor-pointer" {
                                            input type="radio" name="intensity" value=(format!("{:?}", int).to_lowercase()) checked[is_active] class="hidden";
                                            span class={"font-mono-stat text-[10px] px-1.5 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                                 style={@if is_active { "color: var(--ink); background: var(--paper-2); border-color: var(--ink-3)" } @else { "color: var(--muted); border-color: var(--rule); background: var(--paper)" }}
                                                 title=(int.full_name()) { (int.label()) }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Row 2: Stroke rate
                    @if is_work {
                        div {
                            label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Stroke rate" }
                            @let rate = seg.rate.unwrap_or([20, 20]);
                            @let is_range = rate[0] != rate[1];
                            div class="flex items-center gap-2 flex-wrap" {
                                div class="flex items-center gap-1" {
                                    span class="font-mono-stat text-xs" style="color: var(--muted)" { "r" }
                                    input type="range" name="rate_low" min="10" max="50" value=(rate[0]) class="range-warm" style="width: 100px";
                                    span class="font-mono-stat text-xs font-medium w-5 text-center" style="color: var(--ink)" { (rate[0]) }
                                }
                                @if is_range {
                                    div class="flex items-center gap-1" {
                                        span class="font-mono-stat text-[9px]" style="color: var(--muted)" { "to" }
                                        input type="range" name="rate_high" min="10" max="50" value=(rate[1]) class="range-warm" style="width: 100px";
                                        span class="font-mono-stat text-xs font-medium w-5 text-center" style="color: var(--ink)" { (rate[1]) }
                                    }
                                } @else {
                                    input type="hidden" name="rate_high" value=(rate[0]);
                                }
                                label class="flex items-center gap-1 cursor-pointer ml-1" title="Toggle rate range" {
                                    input type="checkbox" name="_range_toggle" checked[is_range] class="hidden";
                                    span class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_range { "font-bold" }}
                                         style={@if is_range { "color: var(--ink); background: var(--paper-2); border-color: var(--ink-3)" } @else { "color: var(--muted); border-color: var(--rule); background: var(--paper)" }} { "range" }
                                }
                            }
                        }
                    }

                    // Row 3: Modifiers
                    @if is_work {
                        div class="flex flex-wrap gap-4 items-start" {
                            // Partial strokes (dropdown)
                            div {
                                label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Partial strokes" }
                                select name="partial" class="input-warm text-xs py-0.5" {
                                    @for s in Slide::ALL {
                                        option value=(slide_value(*s)) selected[seg.partial == Some(*s) || (*s == Slide::Full && seg.partial.is_none())] { (s.label()) }
                                    }
                                }
                            }
                            // Blade
                            div {
                                label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Blade" }
                                div class="flex gap-1" {
                                    @for (val, lbl) in &[("feather", "feather"), ("partial-feather", "partial feather"), ("square", "on square")] {
                                        @let is_active = match *val {
                                            "square" => seg.blade == Some(Blade::Square),
                                            "partial-feather" => seg.blade == Some(Blade::PartialFeather),
                                            _ => seg.blade.is_none() || seg.blade == Some(Blade::Feather),
                                        };
                                        label class="cursor-pointer" {
                                            input type="radio" name="blade" value=(val) checked[is_active] class="hidden";
                                            span class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                                 style={@if is_active { "color: var(--ink); background: var(--paper-2); border-color: var(--ink-3)" } @else { "color: var(--muted); border-color: var(--rule); background: var(--paper)" }} { (lbl) }
                                        }
                                    }
                                }
                            }
                            // Pause (multi-select chips + every-N-strokes)
                            @let pause_csv = seg.pause.iter().map(|p| pause_value(*p)).collect::<Vec<_>>().join(",");
                            div {
                                label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Pause at" }
                                input type="hidden" name="pause_points" value=(pause_csv) data-multiselect="pause";
                                div class="flex flex-wrap gap-1" {
                                    @for pp in PausePoint::ALL {
                                        @let is_active = seg.pause.contains(pp);
                                        label class="cursor-pointer" {
                                            input type="checkbox" value=(pause_value(*pp)) checked[is_active] class="hidden"
                                                  onchange="event.stopPropagation();var h=this.form.querySelector('[data-multiselect=pause]');var vs=[];this.form.querySelectorAll('input[type=checkbox][onchange*=pause]:checked').forEach(function(c){vs.push(c.value)});h.value=vs.join(',');h.dispatchEvent(new Event('change',{bubbles:true}))";
                                            span class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                                 style={@if is_active { "color: var(--ink); background: var(--paper-2); border-color: var(--ink-3)" } @else { "color: var(--muted); border-color: var(--rule); background: var(--paper)" }} { (pp.label()) }
                                        }
                                    }
                                }
                                @if !seg.pause.is_empty() {
                                    div class="flex items-center gap-1 mt-1" {
                                        label class="font-mono-stat text-[9px]" style="color: var(--muted)" { "every" }
                                        input type="number" name="pause_every" min="1" max="20"
                                              value=(seg.pause_every.unwrap_or(1))
                                              class="input-warm text-xs w-14 py-0.5 text-center";
                                        span class="font-mono-stat text-[9px]" style="color: var(--muted)" { "strokes" }
                                    }
                                }
                            }
                            // Drills
                            @let drills_csv = seg.drills.iter().map(|d| hand_drill_value(*d)).collect::<Vec<_>>().join(",");
                            div {
                                label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { "Drills" }
                                input type="hidden" name="drills" value=(drills_csv) data-multiselect="drills";
                                div class="flex flex-wrap gap-1" {
                                    @for hd in HandDrill::ALL {
                                        @let is_active = seg.drills.contains(hd);
                                        label class="cursor-pointer" {
                                            input type="checkbox" value=(hand_drill_value(*hd)) checked[is_active] class="hidden"
                                                  onchange="event.stopPropagation();var h=this.form.querySelector('[data-multiselect=drills]');var vs=[];this.form.querySelectorAll('input[type=checkbox][onchange*=drills]:checked').forEach(function(c){vs.push(c.value)});h.value=vs.join(',');h.dispatchEvent(new Event('change',{bubbles:true}))";
                                            span class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                                 style={@if is_active { "color: var(--ink); background: var(--paper-2); border-color: var(--ink-3)" } @else { "color: var(--muted); border-color: var(--rule); background: var(--paper)" }} { (hd.label()) }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Emphasis / note
                    div {
                        label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" {
                            @if is_work { "Emphasis" } @else { "Notes" }
                        }
                        textarea name="note" rows="1" placeholder={@if is_work { "e.g. connection at the catch" } @else { "e.g. paddle between pieces" }}
                                 class="input-warm text-sm w-full resize-y" { (seg.note) }
                    }
                }
            }
        }
    }
}
