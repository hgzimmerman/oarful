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

pub(super) fn action_buttons(base_url: &str, tl_json: &str, item_id: &str) -> Markup {
    html! {
        div class="flex items-center gap-1" {
            form class="inline" hx-post={(base_url) "/duplicate"} hx-target="#timeline-section" hx-swap="outerHTML" {
                input type="hidden" name="timeline" value=(tl_json);
                input type="hidden" name="dup_id" value=(item_id);
                button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer" style="color: var(--ink-2); border-color: var(--rule); background: var(--paper)" { "Duplicate" }
            }
            form class="inline" hx-post={(base_url) "/delete"} hx-target="#timeline-section" hx-swap="outerHTML" {
                input type="hidden" name="timeline" value=(tl_json);
                input type="hidden" name="delete_id" value=(item_id);
                button type="submit" class="font-mono-stat text-[9px] px-1.5 py-0.5 rounded border cursor-pointer" style="color: var(--bad); border-color: color-mix(in oklch, var(--bad) 30%, var(--rule)); background: var(--paper)" { "Delete" }
            }
        }
    }
}
