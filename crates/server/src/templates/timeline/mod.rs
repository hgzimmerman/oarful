//! Timeline editor + summary templates.

mod block_editor;
mod css;
mod group_editor;
mod helpers;
mod segment_editor;
mod strip;
mod tooltips;

use lineup_db::timeline::{
    self, Block, BlockType, Group, GroupType, ItemDisplayType, Timeline, TimelineItem,
};
use maud::{html, Markup};

use super::history::block_type_css;
use css::{group_type_css, seg_type_css};

// ── Summary (collapsed view) ─────────────────────────────────────────

pub(crate) fn summary(tl: &Timeline, base_url: &str) -> Markup {
    summary_inner(tl, base_url, None, None)
}

pub(crate) fn summary_with_import(tl: &Timeline, base_url: &str, import_url: &str) -> Markup {
    summary_inner(tl, base_url, Some(import_url), None)
}

pub(crate) fn practice_summary(
    tl: &Timeline,
    base_url: &str,
    import_url: &str,
    skip_url: Option<&str>,
) -> Markup {
    summary_inner(tl, base_url, Some(import_url), skip_url)
}

fn summary_inner(
    tl: &Timeline,
    base_url: &str,
    import_url: Option<&str>,
    skip_url: Option<&str>,
) -> Markup {
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
                               hx-get={(base_url) "/edit"}
                               hx-target="#timeline-section"
                               hx-swap="outerHTML" {
                            "edit plan"
                        }
                        @if let Some(url) = import_url {
                            button class="font-mono-stat text-[10.5px] hover:underline cursor-pointer px-1"
                                   style="color: var(--muted); background: none; border: none"
                                   hx-get=(url)
                                   hx-target="body"
                                   hx-swap="beforeend" {
                                "use template"
                            }
                        }
                        @if let Some(url) = skip_url {
                            button class="font-mono-stat text-[10.5px] hover:underline cursor-pointer px-1"
                                   style="color: var(--muted); background: none; border: none"
                                   hx-post=(url)
                                   hx-target="#content" {
                                "skip plan"
                            }
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

// ── Editor (expanded view) ───────────────────────────────────────────

pub(crate) fn editor(tl: &Timeline, base_url: &str, selected_id: Option<&str>) -> Markup {
    editor_inner(tl, base_url, selected_id, true)
}

pub(crate) fn editor_no_animate(
    tl: &Timeline,
    base_url: &str,
    selected_id: Option<&str>,
) -> Markup {
    editor_inner(tl, base_url, selected_id, false)
}

fn editor_inner(tl: &Timeline, base_url: &str, selected_id: Option<&str>, animate: bool) -> Markup {
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

            // Hidden reorder forms
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
            (strip::timeline_strip(tl, base_url, &tl_json, selected_id))

            // Editor for selected item
            @match &selected {
                Sel::Block(block) => { (block_editor::bare_block_editor(block, base_url, &tl_json, animate)) }
                Sel::Group(group) => { (group_editor::group_editor(group, base_url, &tl_json, selected_id, animate)) }
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
