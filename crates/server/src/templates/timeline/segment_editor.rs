//! Segment detail editor — base fields + modifier rows with inheritance.

use lineup_db::timeline::{
    Blade, DurationUnit, Group, HandDrill, Modifier, ModifierCatalogueEntry, PausePoint, Segment,
    Slide,
};
use maud::{html, Markup};

use super::css::chip_style;
use super::helpers::field_label;

/// Computed modifier state for rendering.
enum ModRow<'a> {
    /// Inherited from the parent group, not overridden.
    Inherited(&'a Modifier),
    /// Inherited and overridden locally on this segment.
    Overridden {
        local: &'a Modifier,
        inherited: &'a Modifier,
    },
    /// Added directly on this segment (no inherited counterpart).
    Local(&'a Modifier),
}

fn effective_modifiers<'a>(seg: &'a Segment, group: &'a Group) -> Vec<ModRow<'a>> {
    let mut rows = Vec::new();
    // Group-level modifiers first (inherited or overridden), skip non-cascading
    for gm in group.modifiers.iter().filter(|m| m.cascades()) {
        if let Some(sm) = seg.modifiers.iter().find(|m| m.kind_id() == gm.kind_id()) {
            rows.push(ModRow::Overridden {
                local: sm,
                inherited: gm,
            });
        } else {
            rows.push(ModRow::Inherited(gm));
        }
    }
    // Segment-level modifiers that don't override anything
    for sm in &seg.modifiers {
        if !group
            .modifiers
            .iter()
            .any(|gm| gm.cascades() && gm.kind_id() == sm.kind_id())
        {
            rows.push(ModRow::Local(sm));
        }
    }
    rows
}

/// Segment modifier editor — shown below the segment card when selected.
/// Base fields (duration, intensity, rate) live in the segment card itself.
pub(super) fn segment_editor(
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
    pe: &str,
) -> Markup {
    let is_work = seg.seg_type.is_work();
    let rows = effective_modifiers(seg, group);

    html! {
        // Only work segments have modifiers
        @if is_work {
            (modifier_section(pe, seg, group, base_url, tl_json, &rows))
        }
    }
}

/// Render the modifiers section: inherited rows, local rows, and the add button.
fn modifier_section(
    pe: &str,
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
    rows: &[ModRow],
) -> Markup {
    let inh_count = rows
        .iter()
        .filter(|r| matches!(r, ModRow::Inherited(_)))
        .count();
    let local_count = rows
        .iter()
        .filter(|r| matches!(r, ModRow::Local(_)))
        .count();
    let override_count = rows
        .iter()
        .filter(|r| matches!(r, ModRow::Overridden { .. }))
        .count();

    // Which kinds are already present on this segment?
    let present_kinds: Vec<&str> = rows
        .iter()
        .map(|r| match r {
            ModRow::Inherited(m) | ModRow::Local(m) => m.kind_id(),
            ModRow::Overridden { local, .. } => local.kind_id(),
        })
        .collect();

    html! {
        div class="mt-3 pt-3" style="border-top: 1px dashed var(--rule-2)" {
            div class="flex items-baseline justify-between mb-2" {
                (field_label("Modifiers"))
                span class="font-mono-stat text-[9px]" style="color: var(--muted)" {
                    @if inh_count > 0 { (inh_count) " inherited" }
                    @if override_count > 0 {
                        @if inh_count > 0 { ", " }
                        (override_count) " edited"
                    }
                    @if local_count > 0 {
                        @if inh_count > 0 || override_count > 0 { ", " }
                        (local_count) " added here"
                    }
                    @if rows.is_empty() { "none" }
                }
            }

            div class="space-y-1" {
                @for row in rows {
                    @match row {
                        ModRow::Inherited(m) => {
                            (inherited_row(pe, m, seg, group, base_url, tl_json))
                        }
                        ModRow::Overridden { local, inherited } => {
                            (overridden_row(pe, local, inherited, seg, group, base_url, tl_json))
                        }
                        ModRow::Local(m) => {
                            (local_row(pe, m, seg, group, base_url, tl_json))
                        }
                    }
                }
            }

            // Add modifier picker
            (modifier_picker(pe, seg, group, base_url, tl_json, &present_kinds))
        }
    }
}

/// An inherited modifier row — read-only with "override here" action.
fn inherited_row(
    pe: &str,
    m: &Modifier,
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
) -> Markup {
    html! {
        @let mc = super::css::modifier_kind_color(m.kind_id());
        div class="flex items-center gap-2 px-2 py-1.5 rounded"
             style=(format!("background: color-mix(in oklch, {mc} 5%, var(--paper)); border: 1px solid var(--rule); border-left: 2px solid {mc}")) {
            span class="font-mono-stat text-[9px] tracking-wider uppercase font-semibold w-20 flex-shrink-0"
                 style=(format!("color: {mc}")) { (m.kind_label()) }
            span class="font-mono-stat text-[10px] flex-1" style=(format!("color: {mc}")) {
                (modifier_value_display(m))
            }
            form class="inline" hx-post={(base_url) "/modifier-override"} hx-target="#timeline-section" hx-swap="innerHTML" {
                input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                input type="hidden" name="group_id" value=(group.id);
                input type="hidden" name="segment_id" value=(seg.id);
                input type="hidden" name="selected" value=(seg.id);
                input type="hidden" name="kind" value=(m.kind_id());
                button type="submit" class="font-mono-stat text-[8px] tracking-wider uppercase px-1.5 py-0.5 rounded cursor-pointer"
                       style="color: var(--muted); background: var(--paper); border: 1px solid var(--rule)" { "override here" }
            }
        }
    }
}

/// An overridden modifier — shows edited value with "was..." note and revert.
fn overridden_row(
    pe: &str,
    local: &Modifier,
    inherited: &Modifier,
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
) -> Markup {
    let mc = super::css::modifier_kind_color(local.kind_id());
    html! {
        div class="flex items-center gap-2 px-2 py-1.5 rounded"
             style=(format!("background: color-mix(in oklch, {mc} 5%, var(--paper)); border: 1px solid color-mix(in oklch, {mc} 28%, var(--rule)); border-left: 2px solid {mc}")) {
            span class="font-mono-stat text-[9px] tracking-wider uppercase font-semibold w-20 flex-shrink-0"
                 style=(format!("color: {mc}")) { (local.kind_label()) }
            span class="flex-1 flex items-center gap-2 flex-wrap" {
                span class="font-mono-stat text-[7px] tracking-wider uppercase font-bold px-1 py-px rounded"
                     style=(format!("background: {mc}; color: var(--paper)")) { "EDITED HERE" }
                (modifier_value_editor(pe, local, seg, group, base_url, tl_json))
                span class="font-mono-stat text-[9px] italic" style="color: var(--muted); border-left: 1px solid var(--rule); padding-left: 6px" {
                    "was " (modifier_value_display(inherited))
                }
            }
            form class="inline" hx-post={(base_url) "/modifier-revert"} hx-target="#timeline-section" hx-swap="innerHTML" {
                input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                input type="hidden" name="group_id" value=(group.id);
                input type="hidden" name="segment_id" value=(seg.id);
                input type="hidden" name="selected" value=(seg.id);
                input type="hidden" name="kind" value=(local.kind_id());
                button type="submit" class="font-mono-stat text-[8px] tracking-wider uppercase px-1.5 py-0.5 rounded cursor-pointer"
                       style="color: var(--muted); background: var(--paper); border: 1px solid var(--rule)" title="Revert to inherited value" { "revert" }
            }
        }
    }
}

/// A locally-added modifier — editable with remove button.
fn local_row(
    pe: &str,
    m: &Modifier,
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
) -> Markup {
    let mc = super::css::modifier_kind_color(m.kind_id());
    html! {
        div class="flex items-center gap-2 px-2 py-1.5 rounded"
             style=(format!("background: color-mix(in oklch, {mc} 4%, var(--paper)); border: 1px solid color-mix(in oklch, {mc} 20%, var(--rule)); border-left: 2px solid {mc}")) {
            span class="font-mono-stat text-[9px] tracking-wider uppercase font-semibold w-20 flex-shrink-0"
                 style=(format!("color: {mc}")) { (m.kind_label()) }
            span class="flex-1" {
                (modifier_value_editor(pe, m, seg, group, base_url, tl_json))
            }
            form class="inline" hx-post={(base_url) "/modifier-remove"} hx-target="#timeline-section" hx-swap="innerHTML" {
                input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                input type="hidden" name="group_id" value=(group.id);
                input type="hidden" name="segment_id" value=(seg.id);
                input type="hidden" name="selected" value=(seg.id);
                input type="hidden" name="kind" value=(m.kind_id());
                input type="hidden" name="scope" value="segment";
                button type="submit" class="font-mono-stat text-sm px-1 cursor-pointer"
                       style="color: var(--muted); background: none; border: none" title="Remove" { "\u{00d7}" }
            }
        }
    }
}

/// Read-only display of a modifier's value.
pub(super) fn modifier_value_display(m: &Modifier) -> Markup {
    html! {
        @match m {
            Modifier::Blade { value } => { (value.label()) }
            Modifier::Partial { value } => { (value.label()) }
            Modifier::PauseAt { points, every } => {
                @if points.is_empty() { "(none)" }
                @else {
                    @for (i, p) in points.iter().enumerate() {
                        @if i > 0 { " + " }
                        (p.label())
                    }
                    @if let Some(e) = every { @if *e > 1 { " every " (e) " str" } }
                }
            }
            Modifier::Drills { values } => {
                @if values.is_empty() { "(none)" }
                @else {
                    @for (i, d) in values.iter().enumerate() {
                        @if i > 0 { ", " }
                        (d.label())
                    }
                }
            }
            Modifier::Emphasis { text } => {
                @if text.is_empty() { em style="color: var(--muted)" { "(empty)" } }
                @else { em { (text) } }
            }
            Modifier::RepeatingEmphasis { every, every_unit, count, label } => {
                span class="font-mono-stat text-[10px]" {
                    @if !label.is_empty() { em { (label) } " — " }
                    (count) " strokes every " (every) " " (every_unit.label())
                }
            }
        }
    }
}

/// Interactive editor for a modifier's value — chips, toggles, text input.
fn modifier_value_editor(
    pe: &str,
    m: &Modifier,
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
) -> Markup {
    let update_url = format!("{}/modifier-update", base_url);
    html! {
        @match m {
            Modifier::Blade { value } => {
                div class="flex gap-1 flex-wrap" {
                    @for v in &[Blade::Feather, Blade::PartialFeather, Blade::Square] {
                        @let is_active = value == v;
                        form class="inline" hx-post=(&update_url) hx-target="#timeline-section" hx-swap="innerHTML" {
                            input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                            input type="hidden" name="group_id" value=(group.id);
                            input type="hidden" name="segment_id" value=(seg.id);
                            input type="hidden" name="selected" value=(seg.id);
                            input type="hidden" name="kind" value="blade";
                            input type="hidden" name="scope" value="segment";
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
                            input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                            input type="hidden" name="group_id" value=(group.id);
                            input type="hidden" name="segment_id" value=(seg.id);
                            input type="hidden" name="selected" value=(seg.id);
                            input type="hidden" name="kind" value="partial";
                            input type="hidden" name="scope" value="segment";
                            input type="hidden" name="value" value=(v);
                            button type="submit" class={"font-mono-stat text-[9px] px-1 py-0.5 rounded border cursor-pointer " @if is_active { "font-bold" }}
                                   style=(chip_style(is_active)) { (v.label()) }
                        }
                    }
                }
            }
            Modifier::PauseAt { points, every } => {
                (pause_at_editor(pe, points, *every, seg, group, base_url, tl_json))
            }
            Modifier::Drills { values } => {
                (drills_editor(pe, values, seg, group, base_url, tl_json))
            }
            Modifier::Emphasis { text } => {
                form class="inline flex-1" hx-post=(&update_url) hx-target="#timeline-section" hx-swap="innerHTML" hx-trigger="change" hx-sync="this:replace" onsubmit="return false" {
                    input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                    input type="hidden" name="group_id" value=(group.id);
                    input type="hidden" name="segment_id" value=(seg.id);
                    input type="hidden" name="selected" value=(seg.id);
                    input type="hidden" name="kind" value="emphasis";
                    input type="hidden" name="scope" value="segment";
                    input type="text" name="value" value=(text)
                          class="input-warm text-sm w-full py-0.5"
                          placeholder="e.g. connection at the catch"
                          style="font-family: var(--font-serif-heading); font-style: italic";
                }
            }
            Modifier::RepeatingEmphasis { every, every_unit, count, label } => {
                (repeating_emphasis_editor(pe, *every, *every_unit, *count, label, seg, group, base_url, tl_json))
            }
        }
    }
}

/// Pause-at multi-select editor with frequency input.
fn pause_at_editor(
    pe: &str,
    points: &[PausePoint],
    every: Option<u32>,
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
) -> Markup {
    let update_url = format!("{}/modifier-toggle", base_url);
    html! {
        div class="flex flex-wrap gap-1 items-center" {
            @for pp in PausePoint::ALL {
                @let is_active = points.contains(pp);
                form class="inline" hx-post=(&update_url) hx-target="#timeline-section" hx-swap="innerHTML" {
                    input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                    input type="hidden" name="group_id" value=(group.id);
                    input type="hidden" name="segment_id" value=(seg.id);
                    input type="hidden" name="selected" value=(seg.id);
                    input type="hidden" name="kind" value="pause_at";
                    input type="hidden" name="scope" value="segment";
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
                form class="inline" hx-post={(base_url) "/modifier-update"} hx-target="#timeline-section" hx-swap="innerHTML" hx-trigger="change" hx-sync="this:replace" onsubmit="return false" {
                    input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                    input type="hidden" name="group_id" value=(group.id);
                    input type="hidden" name="segment_id" value=(seg.id);
                    input type="hidden" name="selected" value=(seg.id);
                    input type="hidden" name="kind" value="pause_at";
                    input type="hidden" name="scope" value="segment";
                    input type="hidden" name="subfield" value="every";
                    input type="number" name="value" min="1" max="20"
                          value=(every.unwrap_or(1))
                          class="input-warm text-xs w-10 py-0.5 text-center";
                }
                span class="font-mono-stat text-[9px]" style="color: var(--muted)" { "str" }
            }
        }
    }
}

/// Drills multi-select editor.
fn drills_editor(
    pe: &str,
    values: &[HandDrill],
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
) -> Markup {
    let update_url = format!("{}/modifier-toggle", base_url);
    html! {
        div class="flex flex-wrap gap-1" {
            @for hd in HandDrill::ALL {
                @let is_active = values.contains(hd);
                form class="inline" hx-post=(&update_url) hx-target="#timeline-section" hx-swap="innerHTML" {
                    input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                    input type="hidden" name="group_id" value=(group.id);
                    input type="hidden" name="segment_id" value=(seg.id);
                    input type="hidden" name="selected" value=(seg.id);
                    input type="hidden" name="kind" value="drills";
                    input type="hidden" name="scope" value="segment";
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
}

/// Repeating emphasis compound editor — sentence-shaped inline form.
fn repeating_emphasis_editor(
    pe: &str,
    every: u32,
    every_unit: DurationUnit,
    count: u32,
    label: &str,
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
) -> Markup {
    let update_url = format!("{}/modifier-update", base_url);
    html! {
        form class="flex flex-wrap items-center gap-1.5 font-mono-stat text-[10px]"
             hx-post=(&update_url) hx-target="#timeline-section" hx-swap="innerHTML" hx-trigger="change" hx-sync="this:replace" onsubmit="return false" {
            input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
            input type="hidden" name="group_id" value=(group.id);
            input type="hidden" name="segment_id" value=(seg.id);
            input type="hidden" name="selected" value=(seg.id);
            input type="hidden" name="kind" value="repeating_emphasis";
            input type="hidden" name="scope" value="segment";
            // "every [N]"
            span style="color: var(--muted)" { "every" }
            input type="number" name="re_every" min="1" max="99" value=(every)
                  class="input-warm input-no-spin text-xs w-10 py-0.5 text-center font-medium";
            // "[min] [strokes]"
            select name="re_every_unit" class="input-warm text-xs py-0.5" {
                option value="min" selected[every_unit == DurationUnit::Min] { "min" }
                option value="strokes" selected[every_unit == DurationUnit::Strokes] { "str" }
            }
            span style="color: var(--rule)" { "\u{00b7}" }
            // "do [N]"
            span style="color: var(--muted)" { "do" }
            input type="number" name="re_count" min="1" max="99" value=(count)
                  class="input-warm input-no-spin text-xs w-10 py-0.5 text-center font-medium";
            span style="color: var(--muted)" { "strokes" }
            span style="color: var(--rule)" { "\u{00b7}" }
            // label
            input type="text" name="re_label" value=(label)
                  class="input-warm text-xs py-0.5 flex-1 min-w-[80px]"
                  placeholder="e.g. power"
                  style="font-style: italic";
        }
    }
}

/// The "+ Add modifier" button and picker popover.
fn modifier_picker(
    pe: &str,
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
    present_kinds: &[&str],
) -> Markup {
    let catalogue = lineup_db::timeline::modifier_catalogue();
    html! {
        div x-data="{ open: false }" class="relative mt-2" {
            button "@click"="open = !open"
                   class="font-mono-stat text-[10px] px-2 py-1 rounded cursor-pointer"
                   style="color: var(--muted); background: transparent; border: 1px dashed var(--rule)" {
                "+ Add modifier"
            }
            // Popover
            div x-show="open" "@click.outside"="open = false" x-cloak=""
                class="absolute z-10 rounded-md shadow-lg"
                style="width: 300px; background: var(--paper); border: 1px solid var(--ink); left: 0; bottom: 100%; margin-bottom: 4px" {
                div class="py-1" {
                    (picker_items(pe, &catalogue, present_kinds, seg, group, base_url, tl_json))
                }
                div class="font-mono-stat text-[9px] px-3 py-1.5 flex justify-between"
                     style="border-top: 1px solid var(--rule); color: var(--muted); background: color-mix(in oklch, var(--ink) 3%, var(--paper))" {
                    span { "click to add" }
                    span { (catalogue.len() - present_kinds.len()) " of " (catalogue.len()) " available" }
                }
            }
        }
    }
}

/// Render picker items grouped by category.
fn picker_items(
    pe: &str,
    catalogue: &[ModifierCatalogueEntry],
    present_kinds: &[&str],
    seg: &Segment,
    group: &Group,
    base_url: &str,
    tl_json: &str,
) -> Markup {
    // Group entries by their group field
    let mut groups: Vec<(&str, Vec<&ModifierCatalogueEntry>)> = Vec::new();
    for entry in catalogue {
        if let Some(g) = groups.last_mut().filter(|(name, _)| *name == entry.group) {
            g.1.push(entry);
        } else {
            groups.push((entry.group, vec![entry]));
        }
    }
    html! {
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
                        div class="font-mono-stat text-[10px] mt-0.5" style="color: var(--ink-2)" { (entry.description) }
                    }
                } @else {
                    form class="contents" hx-post={(base_url) "/modifier-add"} hx-target="#timeline-section" hx-swap="innerHTML" {
                        input type="hidden" name="timeline" value=(tl_json);
                    input type="hidden" name="plan_editor" value=(pe);
                        input type="hidden" name="group_id" value=(group.id);
                        input type="hidden" name="segment_id" value=(seg.id);
                        input type="hidden" name="selected" value=(seg.id);
                        input type="hidden" name="kind" value=(entry.kind_id);
                        input type="hidden" name="scope" value="segment";
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
