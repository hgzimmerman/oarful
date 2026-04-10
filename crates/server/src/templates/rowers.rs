//! Roster table with inline-edit support.
//!
//! Rendering is split into [`list_content`] (the page shell + table)
//! and [`static_row`] / [`edit_row`] (the per-row pieces). The two row
//! variants share the same `<tr>` shape so HTMX `outerHTML` swaps land
//! cleanly.

use lineup_db::rower::{
    types::{RowerWeightClass, Side, Skill, Strength},
    Rower,
};
use maud::{html, Markup};

use super::layout::{empty_state, page_header};

pub(crate) fn list_content(rowers: &[Rower]) -> Markup {
    let subtitle = format!("{} active rowers", rowers.len());
    html! {
        (page_header("Rowers", Some(&subtitle)))
        div class="px-8 py-6" {
            @if rowers.is_empty() {
                (empty_state("No rowers on file. Sync the spreadsheet to populate the roster."))
            } @else {
                div class="bg-white rounded-lg shadow overflow-hidden max-w-6xl" {
                    table class="w-full text-sm" {
                        thead class="bg-slate-100 text-left text-xs uppercase text-slate-600" {
                            tr {
                                th class="px-4 py-2" { "Name" }
                                th class="px-4 py-2" { "Weight" }
                                th class="px-4 py-2" { "Skill" }
                                th class="px-4 py-2" { "Strength" }
                                th class="px-4 py-2" { "Side" }
                                th class="px-4 py-2" { "Side str" }
                                th class="px-4 py-2" { "Cox" }
                                th class="px-4 py-2" { "Scull" }
                                th class="px-4 py-2 text-right" { "" }
                            }
                        }
                        tbody {
                            @for r in rowers {
                                (static_row(r))
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Read-only `<tr>` for one rower. Click the Edit button to swap it
/// with [`edit_row`].
pub(crate) fn static_row(r: &Rower) -> Markup {
    html! {
        tr class="border-t border-slate-100 hover:bg-slate-50" {
            td class="px-4 py-2 font-medium text-slate-800" { (r.name) }
            td class="px-4 py-2" { (r.weight_class) }
            td class="px-4 py-2" { (r.skill) }
            td class="px-4 py-2" { (r.strength) }
            td class="px-4 py-2" { (r.side) }
            td class="px-4 py-2 font-mono text-xs" { (r.side_strength) }
            td class="px-4 py-2" {
                @if r.is_designated_cox.as_bool() { "designated" }
                @else if r.can_cox.as_bool() { "yes" }
                @else { "—" }
            }
            td class="px-4 py-2" {
                @if r.can_scull.as_bool() { "yes" } @else { "—" }
            }
            td class="px-4 py-2 text-right" {
                button type="button"
                       class="text-slate-500 hover:text-slate-800 text-xs font-semibold uppercase tracking-wide"
                       hx-get={"/rowers/" (r.id) "/edit"}
                       hx-target="closest tr"
                       hx-swap="outerHTML" {
                    "Edit"
                }
            }
        }
    }
}

/// Editable `<tr>` for one rower. The Save button serialises every
/// input in the same row via `hx-include="closest tr"` so we don't
/// need a wrapping `<form>` element (which would be invalid as a
/// direct child of `<tbody>`).
pub(crate) fn edit_row(r: &Rower, error: Option<&str>) -> Markup {
    let post_url = format!("/rowers/{}", r.id);
    let row_url = format!("/rowers/{}/row", r.id);
    html! {
        tr class="border-t border-slate-200 bg-amber-50" {
            td class="px-4 py-2 font-medium text-slate-800 align-top" {
                (r.name)
                @if let Some(msg) = error {
                    div class="mt-1 text-xs text-red-700" { (msg) }
                }
            }
            td class="px-2 py-2 align-top" {
                (enum_select("weight_class", &[
                    ("Light", RowerWeightClass::Light == r.weight_class),
                    ("Medium", RowerWeightClass::Medium == r.weight_class),
                    ("Heavy", RowerWeightClass::Heavy == r.weight_class),
                ]))
            }
            td class="px-2 py-2 align-top" {
                (enum_select("skill", &[
                    ("Novice", Skill::Novice == r.skill),
                    ("Intermediate", Skill::Intermediate == r.skill),
                    ("Master", Skill::Master == r.skill),
                    ("Expert", Skill::Expert == r.skill),
                ]))
            }
            td class="px-2 py-2 align-top" {
                (enum_select("strength", &[
                    ("Weak", Strength::Weak == r.strength),
                    ("Intermediate", Strength::Intermediate == r.strength),
                    ("Strong", Strength::Strong == r.strength),
                    ("VeryStrong", Strength::VeryStrong == r.strength),
                ]))
            }
            td class="px-2 py-2 align-top" {
                (enum_select("side", &[
                    ("Port", Side::Port == r.side),
                    ("Starboard", Side::Starboard == r.side),
                    ("Either", Side::Either == r.side),
                ]))
            }
            td class="px-2 py-2 align-top" {
                input name="side_strength" type="number" min="0" max="5"
                      value=(r.side_strength)
                      class="w-16 border border-slate-300 rounded px-2 py-1 font-mono text-xs focus:border-slate-500 focus:outline-none";
            }
            td class="px-2 py-2 align-top" {
                (checkbox("can_cox", "can cox", r.can_cox.as_bool()))
                (checkbox("is_designated_cox", "designated", r.is_designated_cox.as_bool()))
            }
            td class="px-2 py-2 align-top" {
                (checkbox("can_scull", "can scull", r.can_scull.as_bool()))
            }
            td class="px-2 py-2 text-right align-top whitespace-nowrap" {
                button type="button"
                       class="bg-emerald-600 hover:bg-emerald-700 text-white text-xs font-semibold px-3 py-1 rounded mr-1"
                       hx-post=(post_url)
                       hx-include="closest tr"
                       hx-target="closest tr"
                       hx-swap="outerHTML" {
                    "Save"
                }
                button type="button"
                       class="text-slate-500 hover:text-slate-800 text-xs font-semibold"
                       hx-get=(row_url)
                       hx-target="closest tr"
                       hx-swap="outerHTML" {
                    "Cancel"
                }
            }
        }
    }
}

fn enum_select(name: &str, options: &[(&str, bool)]) -> Markup {
    html! {
        select name=(name)
               class="border border-slate-300 rounded px-2 py-1 text-xs focus:border-slate-500 focus:outline-none" {
            @for (value, selected) in options {
                @if *selected {
                    option value=(value) selected { (value) }
                } @else {
                    option value=(value) { (value) }
                }
            }
        }
    }
}

fn checkbox(name: &str, label: &str, checked: bool) -> Markup {
    html! {
        label class="flex items-center space-x-1 text-xs text-slate-700" {
            @if checked {
                input type="checkbox" name=(name) checked;
            } @else {
                input type="checkbox" name=(name);
            }
            span { (label) }
        }
    }
}
