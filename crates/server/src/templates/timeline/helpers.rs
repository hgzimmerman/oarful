//! Shared markup helpers used across timeline editor submodules.

use lineup_db::timeline::{self, DurationUnit};
use maud::{html, Markup};

pub(super) fn field_label(text: &str) -> Markup {
    html! {
        label class="font-mono-stat text-[9px] tracking-wider uppercase mb-1 block" style="color: var(--muted)" { (text) }
    }
}

pub(super) fn duration_field(value: f64, unit: DurationUnit) -> Markup {
    html! {
        div {
            (field_label("Duration"))
            div class="flex items-center gap-1" {
                input type="number" name="duration_value" min="0" step="0.5" value=(value) class="input-warm text-sm w-16 py-1";
                select name="duration_unit" class="input-warm text-sm py-1" {
                    option value="min" selected[unit == timeline::DurationUnit::Min] { "min" }
                    option value="meters" selected[unit == timeline::DurationUnit::Meters] { "meters" }
                    option value="strokes" selected[unit == timeline::DurationUnit::Strokes] { "strokes" }
                }
            }
        }
    }
}

/// Compact inline duration field (no label, smaller).
pub(super) fn duration_field_compact(value: f64, unit: DurationUnit) -> Markup {
    html! {
        div class="flex items-center gap-1" {
            span class="font-mono-stat text-[8px] tracking-wider uppercase" style="color: var(--muted)" { "Duration" }
            @let step = match unit { DurationUnit::Meters => "50", DurationUnit::Strokes => "5", _ => "0.5" };
            input type="number" name="duration_value" min="0" step=(step) value=(value) class="input-warm text-xs w-20 py-0.5 text-center";
            select name="duration_unit" class="input-warm text-xs py-0.5" {
                option value="min" selected[unit == timeline::DurationUnit::Min] { "min" }
                option value="meters" selected[unit == timeline::DurationUnit::Meters] { "m" }
                option value="strokes" selected[unit == timeline::DurationUnit::Strokes] { "str" }
            }
        }
    }
}

/// Compact stroke rate field with number inputs (no sliders).
pub(super) fn rate_field_compact(rate: [u8; 2], is_range: bool) -> Markup {
    html! {
        div class="flex items-center gap-1" {
            span class="font-mono-stat text-[8px] tracking-wider uppercase" style="color: var(--muted)" { "Rate" }
            input type="number" name="rate_low" min="10" max="50" value=(rate[0])
                  class="input-warm input-no-spin text-xs w-10 py-0.5 text-center font-medium";
            @if is_range {
                span class="font-mono-stat text-xs" style="color: var(--muted)" { "–" }
                input type="number" name="rate_high" min="10" max="50" value=(rate[1])
                      class="input-warm input-no-spin text-xs w-10 py-0.5 text-center font-medium";
            } @else {
                input type="hidden" name="rate_high" value=(rate[0]);
            }
            label class="flex items-center cursor-pointer" title="Toggle rate range" {
                input type="checkbox" name="_range_toggle" checked[is_range] class="hidden";
                span class={"font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer " @if is_range { "font-bold" }}
                     style=(super::css::chip_style(is_range)) { "range" }
            }
        }
    }
}

pub(super) fn action_buttons(
    base_url: &str,
    tl_json: &str,
    item_id: &str,
    pe: super::PlanEditorState,
) -> Markup {
    html! {
        div class="flex items-center gap-1" {
            form class="inline" hx-post={(base_url) "/duplicate"} hx-target="#timeline-section" hx-swap="innerHTML" {
                input type="hidden" name="timeline" value=(tl_json);
                input type="hidden" name="plan_editor" value=(pe);
                input type="hidden" name="dup_id" value=(item_id);
                button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer" style="color: var(--ink-2); border-color: var(--rule); background: var(--paper)" { "Duplicate" }
            }
            form class="inline" hx-post={(base_url) "/delete"} hx-target="#timeline-section" hx-swap="innerHTML" {
                input type="hidden" name="timeline" value=(tl_json);
                input type="hidden" name="plan_editor" value=(pe);
                input type="hidden" name="delete_id" value=(item_id);
                button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer" style="color: var(--bad); border-color: color-mix(in oklch, var(--bad) 30%, var(--rule)); background: var(--paper)" { "Delete" }
            }
        }
    }
}
