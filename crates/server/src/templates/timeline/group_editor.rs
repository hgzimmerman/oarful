//! Editor for group items (Warmup/Piece) — group fields, segment list, and segment detail.

use lineup_db::timeline::{
    Blade, DurationUnit, Group, GroupType, HandDrill, Intensity, Modifier, PausePoint, RotatePer,
    Slide,
};
use maud::{html, Markup};

use super::css::{chip_style, group_type_css, seg_type_css};
use super::helpers::{action_buttons, duration_field_compact, field_label, rate_field_compact};
use super::segment_editor;

pub(super) fn group_editor(
    group: &Group,
    base_url: &str,
    tl_json: &str,
    selected_id: Option<&str>,
) -> Markup {
    let selected_seg = selected_id.and_then(|sid| group.segments.iter().find(|s| s.id == sid));

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
                    form class="inline" hx-post={(base_url) "/group-patch"} hx-target="#timeline-section" hx-swap="innerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        input type="hidden" name="group_id" value=(group.id);
                        input type="hidden" name="selected" value=(group.id);
                        input type="hidden" name="group_type" value=(other_type);
                        button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer" style="color: var(--ink-2); border-color: var(--rule); background: var(--paper)" { (other_label) }
                    }
                    // Split button — only when repeated
                    @if group.repeat.unwrap_or(1) > 1 {
                        form class="inline" hx-post={(base_url) "/group-split"} hx-target="#timeline-section" hx-swap="innerHTML" {
                            input type="hidden" name="timeline" value=(tl_json);
                            input type="hidden" name="group_id" value=(group.id);
                            button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer"
                                   style="color: var(--ink-2); border-color: var(--rule); background: var(--paper)"
                                   title="Expand repetitions into individual segments" { "Split" }
                        }
                    }
                    (action_buttons(base_url, tl_json, &group.id))
                }
            }

            // Group fields
            form hx-post={(base_url) "/group-patch"} hx-target="#timeline-section" hx-swap="innerHTML" hx-trigger="change" hx-sync="this:replace" {
                input type="hidden" name="timeline" value=(tl_json);
                input type="hidden" name="group_id" value=(group.id);
                input type="hidden" name="selected" value=(group.id);
                div class="flex flex-wrap gap-4 items-start mb-3" {
                    div class="flex-1 min-w-[150px]" {
                        (field_label("Name"))
                        input type="text" name="group_name" value=(group.name) placeholder="e.g. Pick drill" class="input-warm text-sm w-full py-1";
                    }
                    div {
                        (field_label("Repeat"))
                        div class="flex items-center gap-1" {
                            input type="number" name="repeat" min="1" max="20"
                                  value=(group.repeat.unwrap_or(1))
                                  class="input-warm text-xs w-14 py-0.5 text-center";
                            span class="font-mono-stat text-[9px]" style="color: var(--muted)" { "×" }
                        }
                    }
                    div {
                        (field_label("Rotation"))
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

            // Segment cards
            div class="space-y-1.5" data-seg-group-id=(group.id) {
                @for (idx, seg) in group.segments.iter().enumerate() {
                    @let is_sel = selected_id == Some(seg.id.as_str());
                    @let is_work = seg.seg_type.is_work();
                    // Clicking the card background selects/deselects the segment.
                    // A hidden form is submitted via JS; clicks on inputs/buttons are excluded.
                    @let select_form_id = format!("sel-{}", seg.id);
                    form id=(select_form_id) class="hidden"
                         hx-post={(base_url) "/group-patch"}
                         hx-target="#timeline-section"
                         hx-swap="innerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        input type="hidden" name="group_id" value=(group.id);
                        input type="hidden" name="selected" value=(if is_sel { &group.id } else { &seg.id });
                    }
                    div class={"rounded px-2 py-2 cursor-pointer" @if is_sel { " ring-1 ring-ink" }}
                         style={"background: var(--paper-2); border: 1px solid var(--rule)" @if is_sel { "; background: var(--paper); border-color: var(--ink-3)" }}
                         draggable="true"
                         data-drag-id=(seg.id)
                         data-drop-id=(seg.id)
                         data-drag-zone="seglist"
                         onclick=(format!("if(event.target.closest('input,button,select,label,textarea'))return;htmx.trigger(document.getElementById('{}'),'submit')", select_form_id)) {

                        // Top row: drag handle, type badge, segment N of M, badges, delete
                        div class="flex items-center gap-1.5 mb-1" {
                            // Drag handle
                            span class="flex-shrink-0 cursor-grab"
                                 style="display: grid; grid-template-columns: 4px 4px; gap: 2px; width: 14px; padding: 2px 2px; user-select: none"
                                 title="Drag to reorder" {
                                @for _ in 0..6 {
                                    span style="width: 3px; height: 3px; border-radius: 50%; background: var(--rule)" {}
                                }
                            }
                            span class="font-mono-stat text-[9px] px-1 py-px rounded border font-semibold" style=(seg_type_css(seg.seg_type)) { (seg.seg_type.label()) }
                            span class="font-mono-stat text-[10px] font-medium flex-1"
                                 style="color: var(--ink)" {
                                "Segment " (idx + 1) " of " (group.segments.len())
                            }
                            // Modifier indicator with tooltip
                            (modifier_indicator(seg, group))
                            // Delete
                            @if group.segments.len() > 1 {
                                form class="inline" hx-post={(base_url) "/group-delete"} hx-target="#timeline-section" hx-swap="innerHTML" {
                                    input type="hidden" name="timeline" value=(tl_json);
                                    input type="hidden" name="group_id" value=(group.id);
                                    input type="hidden" name="segment_id" value=(seg.id);
                                    button type="submit" class="font-mono-stat text-xs px-1 cursor-pointer"
                                           style="color: var(--bad); background: none; border: none" title="Remove" { "\u{00d7}" }
                                }
                            }
                        }

                        // Inline editable fields: duration, intensity, rate
                        form hx-post={(base_url) "/patch-segment"} hx-target="#timeline-section" hx-swap="innerHTML" hx-trigger="change" hx-sync="this:replace" {
                            input type="hidden" name="timeline" value=(tl_json);
                            input type="hidden" name="group_id" value=(group.id);
                            input type="hidden" name="segment_id" value=(seg.id);
                            input type="hidden" name="selected" value=(selected_id.unwrap_or(&group.id));
                            div class="flex flex-wrap gap-3 items-center" {
                                (duration_field_compact(seg.duration.value, seg.duration.unit))
                                @if is_work {
                                    // Intensity dropdown
                                    div class="flex items-center gap-1" {
                                        span class="font-mono-stat text-[8px] tracking-wider uppercase" style="color: var(--muted)" { "Intensity" }
                                        select name="intensity" class="input-warm text-xs py-0.5" {
                                            @for int in Intensity::ALL {
                                                option value=(int) selected[seg.intensity == Some(*int)] title=(int.full_name()) { (int.label()) }
                                            }
                                        }
                                    }
                                    // Stroke rate
                                    @let rate = seg.rate.unwrap_or([20, 20]);
                                    @let is_range = rate[0] != rate[1];
                                    (rate_field_compact(rate, is_range))
                                }
                                @if !is_work {
                                    // Note for rest/turn segments inline
                                    div class="flex-1 min-w-[120px]" {
                                        input type="text" name="note" value=(seg.note)
                                              placeholder="note" class="input-warm text-xs w-full py-0.5"
                                              style="color: var(--muted); font-style: italic";
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Add segment buttons
            div class="flex gap-1 mt-1" {
                @for (st, label) in &[("work", "+ Segment"), ("rest", "+ Rest"), ("turn", "+ Turn")] {
                    form class="inline" hx-post={(base_url) "/group-add"} hx-target="#timeline-section" hx-swap="innerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                        input type="hidden" name="group_id" value=(group.id);
                        input type="hidden" name="seg_type" value=(st);
                        button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer hover:opacity-80"
                               style="color: var(--ink-2); border-color: var(--rule); background: var(--paper)" { (label) }
                    }
                }
            }

            // Group-level modifiers
            @if selected_seg.is_none() {
                (group_modifiers_section(group, base_url, tl_json))
            }

            // Selected segment detail editor
            @if let Some(seg) = selected_seg {
                (segment_editor::segment_editor(seg, group, base_url, tl_json))
            }
        }
    }
}

/// Small indicator dot/icon on a segment card showing modifier count,
/// with a detailed tooltip listing all effective modifiers.
fn modifier_indicator(
    seg: &lineup_db::timeline::Segment,
    group: &lineup_db::timeline::Group,
) -> Markup {
    // Compute effective modifiers (only cascading ones)
    let mut lines: Vec<String> = Vec::new();
    for gm in group.modifiers.iter().filter(|m| m.cascades()) {
        if let Some(sm) = seg.modifiers.iter().find(|m| m.kind_id() == gm.kind_id()) {
            // Overridden
            let summary = sm.summary_label();
            let was = gm.summary_label();
            lines.push(format!(
                "{}: {} (edited, was {})",
                sm.kind_label(),
                summary,
                was
            ));
        } else {
            // Inherited
            let summary = gm.summary_label();
            if summary.is_empty() {
                lines.push(format!("{} (inherited)", gm.kind_label()));
            } else {
                lines.push(format!("{}: {} (inherited)", gm.kind_label(), summary));
            }
        }
    }
    for sm in &seg.modifiers {
        if !group
            .modifiers
            .iter()
            .any(|gm| gm.cascades() && gm.kind_id() == sm.kind_id())
        {
            let summary = sm.summary_label();
            if summary.is_empty() {
                lines.push(sm.kind_label().to_string());
            } else {
                lines.push(format!("{}: {}", sm.kind_label(), summary));
            }
        }
    }

    let total = lines.len();
    if total == 0 {
        return html! {};
    }

    let tooltip = lines.join("\n");

    html! {
        span class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded-full cursor-default"
             style="background: color-mix(in oklch, var(--accent) 14%, var(--paper)); color: var(--accent)"
             title=(tooltip) {
            (total) " modifier" @if total != 1 { "s" }
        }
    }
}

/// Inline value editor for a group-level modifier.
fn group_modifier_value_editor(
    m: &Modifier,
    group: &Group,
    base_url: &str,
    tl_json: &str,
) -> Markup {
    let update_url = format!("{}/modifier-update", base_url);
    // Hidden fields shared by all forms in this editor
    macro_rules! hidden_fields {
        ($kind:expr) => {
            html! {
                input type="hidden" name="timeline" value=(tl_json);
                input type="hidden" name="group_id" value=(group.id);
                input type="hidden" name="selected" value=(group.id);
                input type="hidden" name="kind" value=($kind);
                input type="hidden" name="scope" value="group";
            }
        };
    }
    html! {
        @match m {
            Modifier::Blade { value } => {
                div class="flex gap-1 flex-wrap" {
                    @for v in &[Blade::Feather, Blade::PartialFeather, Blade::Square] {
                        @let is_active = value == v;
                        form class="inline" hx-post=(&update_url) hx-target="#timeline-section" hx-swap="innerHTML" {
                            (hidden_fields!("blade"))
                            input type="hidden" name="value" value=(serde_json::to_string(v).unwrap_or_default().trim_matches('"'));
                            button type="submit" class={"font-mono-stat text-[10px] px-1.5 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                   style=(chip_style(is_active)) { (v.label()) }
                        }
                    }
                }
            }
            Modifier::Partial { value } => {
                div class="flex gap-1 flex-wrap" {
                    @for v in Slide::ALL {
                        @let is_active = value == v;
                        form class="inline" hx-post=(&update_url) hx-target="#timeline-section" hx-swap="innerHTML" {
                            (hidden_fields!("partial"))
                            input type="hidden" name="value" value=(v);
                            button type="submit" class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                   style=(chip_style(is_active)) { (v.label()) }
                        }
                    }
                }
            }
            Modifier::PauseAt { points, every } => {
                div class="flex flex-wrap gap-1 items-center" {
                    @let toggle_url = format!("{}/modifier-toggle", base_url);
                    @for pp in PausePoint::ALL {
                        @let is_active = points.contains(pp);
                        form class="inline" hx-post=(&toggle_url) hx-target="#timeline-section" hx-swap="innerHTML" {
                            (hidden_fields!("pause_at"))
                            input type="hidden" name="value" value=(pp);
                            button type="submit" class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                   style=(if is_active {
                                       "color: var(--accent); background: color-mix(in oklch, var(--accent) 18%, var(--paper)); border-color: var(--accent)"
                                   } else {
                                       chip_style(false)
                                   }) { (pp.label()) }
                        }
                    }
                    @if !points.is_empty() {
                        span class="font-mono-stat text-[9px] ml-1" style="color: var(--muted)" { "every" }
                        form class="inline" hx-post=(&update_url) hx-target="#timeline-section" hx-swap="innerHTML" hx-trigger="change" hx-sync="this:replace" {
                            (hidden_fields!("pause_at"))
                            input type="hidden" name="subfield" value="every";
                            input type="number" name="value" min="1" max="20"
                                  value=(every.unwrap_or(1))
                                  class="input-warm text-xs w-10 py-0.5 text-center";
                        }
                        span class="font-mono-stat text-[9px]" style="color: var(--muted)" { "str" }
                    }
                }
            }
            Modifier::Drills { values } => {
                div class="flex flex-wrap gap-1" {
                    @let toggle_url = format!("{}/modifier-toggle", base_url);
                    @for hd in HandDrill::ALL {
                        @let is_active = values.contains(hd);
                        form class="inline" hx-post=(&toggle_url) hx-target="#timeline-section" hx-swap="innerHTML" {
                            (hidden_fields!("drills"))
                            input type="hidden" name="value" value=(hd);
                            button type="submit" class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                   style=(if is_active {
                                       "color: var(--accent); background: color-mix(in oklch, var(--accent) 18%, var(--paper)); border-color: var(--accent)"
                                   } else {
                                       chip_style(false)
                                   }) { (hd.label()) }
                        }
                    }
                }
            }
            Modifier::Emphasis { text } => {
                form class="inline flex-1" hx-post=(&update_url) hx-target="#timeline-section" hx-swap="innerHTML" hx-trigger="change" hx-sync="this:replace" {
                    (hidden_fields!("emphasis"))
                    input type="text" name="value" value=(text)
                          class="input-warm text-sm w-full py-0.5"
                          placeholder="e.g. connection at the catch"
                          style="font-style: italic";
                }
            }
            Modifier::RepeatingEmphasis { every, every_unit, count, label } => {
                form class="flex flex-wrap items-center gap-1.5 font-mono-stat text-[10px]"
                     hx-post=(&update_url) hx-target="#timeline-section" hx-swap="innerHTML" hx-trigger="change" hx-sync="this:replace" {
                    (hidden_fields!("repeating_emphasis"))
                    span style="color: var(--muted)" { "every" }
                    input type="number" name="re_every" min="1" max="99" value=(every)
                          class="input-warm input-no-spin text-xs w-10 py-0.5 text-center font-medium";
                    select name="re_every_unit" class="input-warm text-xs py-0.5" {
                        option value="min" selected[*every_unit == lineup_db::timeline::DurationUnit::Min] { "min" }
                        option value="strokes" selected[*every_unit == lineup_db::timeline::DurationUnit::Strokes] { "str" }
                    }
                    span style="color: var(--rule)" { "\u{00b7}" }
                    span style="color: var(--muted)" { "do" }
                    input type="number" name="re_count" min="1" max="99" value=(count)
                          class="input-warm input-no-spin text-xs w-10 py-0.5 text-center font-medium";
                    span style="color: var(--muted)" { "strokes" }
                    span style="color: var(--rule)" { "\u{00b7}" }
                    input type="text" name="re_label" value=(label)
                          class="input-warm text-xs py-0.5 flex-1 min-w-[80px]"
                          placeholder="e.g. power"
                          style="font-style: italic";
                }
            }
        }
    }
}

/// Group-level modifiers section with add/remove.
fn group_modifiers_section(group: &Group, base_url: &str, tl_json: &str) -> Markup {
    let catalogue = lineup_db::timeline::modifier_catalogue();
    let present_kinds: Vec<&str> = group.modifiers.iter().map(|m| m.kind_id()).collect();

    html! {
        div class="mt-3 pt-3" style="border-top: 1px dashed var(--rule-2)" {
            div class="flex items-baseline justify-between mb-2" {
                (field_label("Modifiers on this set"))
                @if group.modifiers.is_empty() {
                    span class="font-mono-stat text-[9px] italic" style="color: var(--muted)" {
                        "none — every rep & segment runs as defined"
                    }
                } @else {
                    span class="font-mono-stat text-[9px]" style="color: var(--muted)" {
                        (group.modifiers.len())
                        // Count how many segments have overrides
                        @let override_count = group.segments.iter()
                            .filter(|s| s.modifiers.iter().any(|sm| group.modifiers.iter().any(|gm| gm.kind_id() == sm.kind_id())))
                            .count();
                        @if override_count > 0 {
                            " · " (override_count) " edited below"
                        }
                    }
                }
            }

            // Modifier rows (editable)
            div class="space-y-1" {
                @for m in &group.modifiers {
                    div class="flex items-center gap-2 px-2 py-1.5 rounded"
                         style="background: color-mix(in oklch, var(--accent) 4%, var(--paper)); border: 1px solid color-mix(in oklch, var(--accent) 20%, var(--rule)); border-left: 2px solid var(--accent)" {
                        span class="font-mono-stat text-[9px] tracking-wider uppercase font-semibold w-20 flex-shrink-0"
                             style="color: var(--ink-2)" { (m.kind_label()) }
                        span class="flex-1" {
                            (group_modifier_value_editor(m, group, base_url, tl_json))
                        }
                        // Show which segments have overrides
                        @let overridden_segs: Vec<&str> = group.segments.iter()
                            .filter(|s| s.modifiers.iter().any(|sm| sm.kind_id() == m.kind_id()))
                            .map(|s| s.seg_type.label())
                            .collect();
                        @if !overridden_segs.is_empty() {
                            span class="font-mono-stat text-[8px] italic" style="color: var(--cox)" {
                                "edited on " (overridden_segs.join(", "))
                            }
                        }
                        form class="inline" hx-post={(base_url) "/modifier-remove"} hx-target="#timeline-section" hx-swap="innerHTML" {
                            input type="hidden" name="timeline" value=(tl_json);
                            input type="hidden" name="group_id" value=(group.id);
                            input type="hidden" name="selected" value=(group.id);
                            input type="hidden" name="kind" value=(m.kind_id());
                            input type="hidden" name="scope" value="group";
                            button type="submit" class="font-mono-stat text-sm px-1 cursor-pointer"
                                   style="color: var(--muted); background: none; border: none" title="Remove" { "\u{00d7}" }
                        }
                    }
                }
            }

            // Add modifier picker
            div x-data="{ open: false }" class="relative mt-2" {
                button "@click"="open = !open"
                       class="font-mono-stat text-[10px] px-2 py-1 rounded cursor-pointer"
                       style="color: var(--muted); background: transparent; border: 1px dashed var(--rule)" {
                    "+ Add modifier (applies to all segments/repetitions)"
                }
                div x-show="open" "@click.outside"="open = false" x-cloak=""
                    class="absolute z-10 rounded-md shadow-lg"
                    style="width: 300px; background: var(--paper); border: 1px solid var(--ink); left: 0; bottom: 100%; margin-bottom: 4px" {
                    (group_picker_items(&catalogue, &present_kinds, group, base_url, tl_json))
                    div class="font-mono-stat text-[9px] px-3 py-1.5 flex justify-between"
                         style="border-top: 1px solid var(--rule); color: var(--muted); background: color-mix(in oklch, var(--ink) 3%, var(--paper))" {
                        span { "click to add" }
                        span { (catalogue.len() - present_kinds.len()) " of " (catalogue.len()) " available" }
                    }
                }
            }
        }
    }
}

fn group_picker_items(
    catalogue: &[lineup_db::timeline::ModifierCatalogueEntry],
    present_kinds: &[&str],
    group: &Group,
    base_url: &str,
    tl_json: &str,
) -> Markup {
    let mut groups: Vec<(&str, Vec<&lineup_db::timeline::ModifierCatalogueEntry>)> = Vec::new();
    for entry in catalogue {
        if let Some(g) = groups.last_mut().filter(|(name, _)| *name == entry.group) {
            g.1.push(entry);
        } else {
            groups.push((entry.group, vec![entry]));
        }
    }
    html! {
        div class="py-1" {
            @for (group_name, entries) in &groups {
                div class="font-mono-stat text-[8px] tracking-widest uppercase px-3 pt-2 pb-1"
                     style="color: var(--muted)" { (group_name) }
                @for entry in entries {
                    @let is_used = present_kinds.contains(&entry.kind_id);
                    @if is_used {
                        div class="px-3 py-1.5 opacity-40 cursor-not-allowed" {
                            div class="flex items-center justify-between" {
                                span class="font-serif-heading text-sm" style="color: var(--ink)" { (entry.name) }
                                span class="font-mono-stat text-[8px] tracking-wider uppercase px-1 py-px rounded"
                                     style="color: var(--ink-3); border: 1px solid var(--rule)" { "added" }
                            }
                        }
                    } @else {
                        form class="contents" hx-post={(base_url) "/modifier-add"} hx-target="#timeline-section" hx-swap="innerHTML" {
                            input type="hidden" name="timeline" value=(tl_json);
                            input type="hidden" name="group_id" value=(group.id);
                            input type="hidden" name="selected" value=(group.id);
                            input type="hidden" name="kind" value=(entry.kind_id);
                            input type="hidden" name="scope" value="group";
                            button type="submit" class="w-full text-left px-3 py-1.5 cursor-pointer"
                                   style="background: none; border: none; border-left: 2px solid transparent"
                                   onmouseover="this.style.background='color-mix(in oklch, var(--accent) 8%, var(--paper))';this.style.borderLeftColor='var(--accent)'"
                                   onmouseout="this.style.background='none';this.style.borderLeftColor='transparent'" {
                                div class="flex items-center justify-between" {
                                    span class="font-serif-heading text-sm" style="color: var(--ink)" { (entry.name) }
                                    span class="font-mono-stat text-[8px] tracking-wider uppercase px-1 py-px rounded"
                                         style="color: var(--muted); border: 1px solid var(--rule)" { (entry.value_shape) }
                                }
                                div class="font-mono-stat text-[10px] mt-0.5" style="color: var(--ink-2)" { (entry.description) }
                            }
                        }
                    }
                }
            }
        }
    }
}
