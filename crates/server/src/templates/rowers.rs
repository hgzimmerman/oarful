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
use crate::handlers::rowers::RowerDetail;

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
            td class="px-4 py-2 text-right whitespace-nowrap" {
                a href={"/rowers/" (r.id)}
                  hx-get={"/rowers/" (r.id)}
                  hx-target="#content"
                  hx-push-url="true"
                  class="text-slate-500 hover:text-slate-800 text-xs font-semibold uppercase tracking-wide mr-3 cursor-pointer" {
                    "Details"
                }
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

// =====================================================================
// Per-rower detail page + affinity sections
// =====================================================================

/// `GET /rowers/{id}` page body. Composed of an attribute summary, a
/// seat-affinities section, and a pair-affinities section. The two
/// affinity sections are also exposed as standalone partials so the
/// CRUD handlers can return just the affected `<section>` for HTMX
/// `outerHTML` swaps.
pub(crate) fn detail_content(detail: &RowerDetail) -> Markup {
    let r = &detail.rower;
    let subtitle = format!(
        "{} · {} · {} · {}",
        r.weight_class, r.skill, r.strength, r.side,
    );
    html! {
        (page_header(&r.name, Some(&subtitle)))
        div class="px-8 py-6 max-w-4xl space-y-6" {
            (attribute_summary(r))
            (seat_affinities_section(detail, None))
            (pair_affinities_section(detail, None))
        }
    }
}

fn attribute_summary(r: &Rower) -> Markup {
    html! {
        section class="bg-white rounded-lg shadow p-6" {
            div class="flex items-start justify-between mb-4" {
                h2 class="text-lg font-bold text-slate-800" { "Attributes" }
                a href="/rowers"
                  hx-get="/rowers"
                  hx-target="#content"
                  hx-push-url="true"
                  class="text-sm text-slate-500 hover:text-slate-800" {
                    "← back to roster"
                }
            }
            dl class="grid grid-cols-2 md:grid-cols-4 gap-3 text-sm" {
                (kv("Weight", &r.weight_class.to_string()))
                (kv("Skill", &r.skill.to_string()))
                (kv("Strength", &r.strength.to_string()))
                (kv("Side", &format!("{} ({})", r.side, r.side_strength)))
                (kv("Can cox", if r.can_cox.as_bool() { "yes" } else { "—" }))
                (kv("Designated", if r.is_designated_cox.as_bool() { "yes" } else { "—" }))
                (kv("Can scull", if r.can_scull.as_bool() { "yes" } else { "—" }))
                (kv("Active", if r.active.as_bool() { "yes" } else { "no" }))
            }
            p class="text-xs text-slate-500 mt-4" {
                "Edit attributes from the roster's inline edit row."
            }
        }
    }
}

fn kv(label: &str, value: &str) -> Markup {
    html! {
        div class="bg-slate-50 rounded p-2" {
            div class="text-xs text-slate-500 uppercase tracking-wide" { (label) }
            div class="font-medium text-slate-800" { (value) }
        }
    }
}

/// Seat-affinities section. Standalone so the CRUD handlers can return
/// it as their HTMX response (`outerHTML` swap on `#seat-affinities`).
pub(crate) fn seat_affinities_section(
    detail: &RowerDetail,
    error: Option<&str>,
) -> Markup {
    let r = &detail.rower;
    let upsert_url = format!("/rowers/{}/seat-affinity", r.id);
    let delete_url = format!("/rowers/{}/seat-affinity/delete", r.id);
    html! {
        section #seat-affinities class="bg-white rounded-lg shadow p-6" {
            div class="flex items-center justify-between mb-3" {
                h2 class="text-lg font-bold text-slate-800" { "Seat preferences" }
                span class="text-xs text-slate-500" {
                    "Per-seat reward / penalty (S3)"
                }
            }
            @if let Some(msg) = error {
                div class="mb-3 text-xs text-red-700 bg-red-50 border-l-4 border-red-500 px-3 py-2 rounded" {
                    (msg)
                }
            }
            @if detail.seat_affinities.is_empty() {
                div class="text-sm text-slate-500 italic mb-3" { "No seat preferences on file." }
            } @else {
                table class="w-full text-sm mb-3" {
                    thead class="text-left text-xs uppercase text-slate-500" {
                        tr {
                            th class="py-1 w-24" { "Seat" }
                            th class="py-1 w-24" { "Weight" }
                            th class="py-1" { "" }
                        }
                    }
                    tbody {
                        @for aff in &detail.seat_affinities {
                            tr class="border-t border-slate-100" {
                                td class="py-1 font-mono" { "s" (aff.seat_position) }
                                td class="py-1 font-mono" { (aff.weight) }
                                td class="py-1 text-right" {
                                    button type="button"
                                           class="text-xs text-red-600 hover:text-red-800"
                                           hx-post=(delete_url)
                                           hx-vals={"{\"seat_position\": " (aff.seat_position) "}"}
                                           hx-target="#seat-affinities"
                                           hx-swap="outerHTML" {
                                        "Delete"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Add form: a tiny inline upsert. Submitting an existing
            // seat overwrites its weight (the db helper is an upsert).
            form hx-post=(upsert_url)
                 hx-target="#seat-affinities"
                 hx-swap="outerHTML"
                 class="flex items-end space-x-2 pt-3 border-t border-slate-200" {
                div {
                    label for="seat_position" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Seat" }
                    select id="seat_position" name="seat_position"
                           class="border border-slate-300 rounded px-2 py-1 text-sm" {
                        @for s in 1..=8i32 {
                            option value=(s) { "s" (s) }
                        }
                    }
                }
                div {
                    label for="seat_weight" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Weight" }
                    input id="seat_weight" name="weight" type="number" min="-5" max="5" value="3"
                          class="w-20 border border-slate-300 rounded px-2 py-1 font-mono text-sm";
                }
                button type="submit"
                       class="bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold px-3 py-1.5 rounded" {
                    "Add / update"
                }
                p class="text-xs text-slate-500 ml-auto self-center" {
                    "−5..−1 = avoid · +1..+5 = prefer · 0 forbidden"
                }
            }
        }
    }
}

/// Pair-affinities section. Standalone so the CRUD handlers can swap
/// it via `outerHTML` on `#pair-affinities`.
pub(crate) fn pair_affinities_section(
    detail: &RowerDetail,
    error: Option<&str>,
) -> Markup {
    let r = &detail.rower;
    let upsert_url = format!("/rowers/{}/pair-affinity", r.id);
    let delete_url = format!("/rowers/{}/pair-affinity/delete", r.id);
    let lookup = |id: lineup_db::rower::types::RowerId| -> &str {
        if id == r.id {
            r.name.as_str()
        } else {
            detail
                .other_rowers
                .iter()
                .find(|o| o.id == id)
                .map(|o| o.name.as_str())
                .unwrap_or("<unknown>")
        }
    };
    html! {
        section #pair-affinities class="bg-white rounded-lg shadow p-6" {
            div class="flex items-center justify-between mb-3" {
                h2 class="text-lg font-bold text-slate-800" { "Pair affinities" }
                span class="text-xs text-slate-500" {
                    "Same-partition reward / penalty (S2)"
                }
            }
            @if let Some(msg) = error {
                div class="mb-3 text-xs text-red-700 bg-red-50 border-l-4 border-red-500 px-3 py-2 rounded" {
                    (msg)
                }
            }
            @if detail.pair_affinities.is_empty() {
                div class="text-sm text-slate-500 italic mb-3" { "No pair affinities on file." }
            } @else {
                table class="w-full text-sm mb-3" {
                    thead class="text-left text-xs uppercase text-slate-500" {
                        tr {
                            th class="py-1" { "Partner" }
                            th class="py-1 w-24" { "Weight" }
                            th class="py-1" { "" }
                        }
                    }
                    tbody {
                        @for aff in &detail.pair_affinities {
                            @let partner_id = if aff.rower_a_id == r.id { aff.rower_b_id } else { aff.rower_a_id };
                            tr class="border-t border-slate-100" {
                                td class="py-1" { (lookup(partner_id)) }
                                td class="py-1 font-mono" { (aff.weight) }
                                td class="py-1 text-right" {
                                    button type="button"
                                           class="text-xs text-red-600 hover:text-red-800"
                                           hx-post=(delete_url)
                                           hx-vals={"{\"partner_id\": " (partner_id) "}"}
                                           hx-target="#pair-affinities"
                                           hx-swap="outerHTML" {
                                        "Delete"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            form hx-post=(upsert_url)
                 hx-target="#pair-affinities"
                 hx-swap="outerHTML"
                 class="flex items-end space-x-2 pt-3 border-t border-slate-200" {
                div class="flex-grow" {
                    label for="partner_id" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Partner" }
                    select id="partner_id" name="partner_id"
                           class="w-full border border-slate-300 rounded px-2 py-1 text-sm" {
                        @for o in &detail.other_rowers {
                            option value=(o.id) { (o.name) }
                        }
                    }
                }
                div {
                    label for="pair_weight" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Weight" }
                    input id="pair_weight" name="weight" type="number" min="-5" max="5" value="3"
                          class="w-20 border border-slate-300 rounded px-2 py-1 font-mono text-sm";
                }
                button type="submit"
                       class="bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold px-3 py-1.5 rounded" {
                    "Add / update"
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
