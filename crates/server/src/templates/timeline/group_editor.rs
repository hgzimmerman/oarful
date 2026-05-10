//! Editor for group items (Warmup/Piece) — group fields, segment list, and segment detail.

use lineup_db::timeline::{DurationUnit, Group, GroupType, RotatePer, Slide};
use maud::{html, Markup};

use super::css::{group_type_css, seg_type_css};
use super::helpers::{action_buttons, field_label};
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
                    (action_buttons(base_url, tl_json, &group.id))
                }
            }

            // Group fields
            form hx-post={(base_url) "/group-patch"} hx-target="#timeline-section" hx-swap="innerHTML" hx-trigger="change" {
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
                        // Drag handle
                        span class="flex-shrink-0 cursor-grab"
                             style="display: grid; grid-template-columns: 4px 4px; gap: 2px; width: 14px; padding: 2px 2px; user-select: none"
                             title="Drag to reorder" {
                            @for _ in 0..6 {
                                span style="width: 3px; height: 3px; border-radius: 50%; background: var(--rule)" {}
                            }
                        }
                        // Click to select
                        form class="flex-1 flex items-center gap-2 min-w-0"
                             hx-post={(base_url) "/group-patch"} hx-target="#timeline-section" hx-swap="innerHTML" {
                            input type="hidden" name="timeline" value=(tl_json);
                            input type="hidden" name="group_id" value=(group.id);
                            input type="hidden" name="selected" value=(seg.id);
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
                        // Delete segment
                        @if group.segments.len() > 1 {
                            form class="inline" hx-post={(base_url) "/group-delete"} hx-target="#timeline-section" hx-swap="innerHTML" {
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
                    form class="inline" hx-post={(base_url) "/group-add"} hx-target="#timeline-section" hx-swap="innerHTML" {
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
                (segment_editor::segment_editor(seg, group, base_url, tl_json))
            }
        }
    }
}
