//! Rower roster list + per-rower detail page with attribute editing.
//!
//! The list view is read-only — each row has a Details link that
//! navigates to the detail page where attributes, seat affinities,
//! and pair affinities are editable via HTMX section swaps.

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
                                th class="px-4 py-2" { "Form" }
                                th class="px-4 py-2" { "Strength" }
                                th class="px-4 py-2" { "Side" }
                                th class="px-4 py-2" { "Side str" }
                                th class="px-4 py-2" { "Cox" }
                                th class="px-4 py-2" { "Scull" }
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

/// Read-only `<tr>` for one rower with a Details link to the full
/// detail/edit page.
fn static_row(r: &Rower) -> Markup {
    html! {
        tr class="border-t border-slate-100 hover:bg-slate-50" {
            td class="px-4 py-2 font-medium" {
                a href={"/rowers/" (r.id)}
                  hx-get={"/rowers/" (r.id)}
                  hx-target="#content"
                  hx-push-url="true"
                  class="text-blue-700 hover:text-blue-900 underline" {
                    (r.name)
                }
            }
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
            (attribute_section(r, None))
            (seat_affinities_section(detail, None))
            (pair_affinities_section(detail, None))
        }
    }
}

/// Read-only attribute display with an Edit button that swaps to an
/// inline form. The section has id `#attributes` for HTMX `outerHTML`.
pub(crate) fn attribute_section(r: &Rower, error: Option<&str>) -> Markup {
    let edit_url = format!("/rowers/{}/edit-attributes", r.id);
    html! {
        section #attributes class="bg-white rounded-lg shadow p-6" {
            div class="flex items-start justify-between mb-4" {
                h2 class="text-lg font-bold text-slate-800" { "Attributes" }
                div class="flex items-center gap-3" {
                    button type="button"
                           class="text-sm text-slate-500 hover:text-slate-800 font-semibold uppercase tracking-wide"
                           hx-get=(edit_url)
                           hx-target="#attributes"
                           hx-swap="outerHTML" {
                        "Edit"
                    }
                    a href="/rowers"
                      hx-get="/rowers"
                      hx-target="#content"
                      hx-push-url="true"
                      class="text-sm text-slate-500 hover:text-slate-800" {
                        "← back to roster"
                    }
                }
            }
            @if let Some(msg) = error {
                div class="mb-3 text-xs text-red-700 bg-red-50 border-l-4 border-red-500 px-3 py-2 rounded" {
                    (msg)
                }
            }
            dl class="grid grid-cols-2 md:grid-cols-4 gap-3 text-sm" {
                (kv("Weight", &r.weight_class.to_string()))
                (kv("Form", &r.skill.to_string()))
                (kv("Strength", &r.strength.to_string()))
                (kv("Side", &format!("{} ({})", r.side, r.side_strength)))
                (kv("Can cox", if r.can_cox.as_bool() { "yes" } else { "—" }))
                (kv("Designated", if r.is_designated_cox.as_bool() { "yes" } else { "—" }))
                (kv("Can scull", if r.can_scull.as_bool() { "yes" } else { "—" }))
                (kv("Active", if r.active.as_bool() { "yes" } else { "no" }))
            }
        }
    }
}

/// Editable attribute form. Save posts to `/rowers/{id}` and the
/// handler returns a fresh `attribute_section` for the HTMX swap.
pub(crate) fn attribute_edit_section(r: &Rower, error: Option<&str>) -> Markup {
    let post_url = format!("/rowers/{}", r.id);
    let cancel_url = format!("/rowers/{}/attributes", r.id);
    html! {
        section #attributes class="bg-white rounded-lg shadow p-6 bg-amber-50/50" {
            div class="flex items-start justify-between mb-4" {
                h2 class="text-lg font-bold text-slate-800" { "Edit attributes" }
                div class="flex items-center gap-2" {
                    button type="button"
                           class="bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold px-3 py-1.5 rounded"
                           hx-post=(post_url)
                           hx-include="#attributes"
                           hx-target="#attributes"
                           hx-swap="outerHTML" {
                        "Save"
                    }
                    button type="button"
                           class="text-slate-500 hover:text-slate-800 text-sm font-semibold"
                           hx-get=(cancel_url)
                           hx-target="#attributes"
                           hx-swap="outerHTML" {
                        "Cancel"
                    }
                }
            }
            @if let Some(msg) = error {
                div class="mb-3 text-xs text-red-700 bg-red-50 border-l-4 border-red-500 px-3 py-2 rounded" {
                    (msg)
                }
            }
            div class="grid grid-cols-2 md:grid-cols-4 gap-3 text-sm" {
                div {
                    label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Weight" }
                    (enum_select("weight_class", &[
                        ("Light", "Lightweight", RowerWeightClass::Light == r.weight_class),
                        ("Medium", "Middleweight", RowerWeightClass::Medium == r.weight_class),
                        ("Heavy", "Heavyweight", RowerWeightClass::Heavy == r.weight_class),
                    ]))
                }
                div {
                    label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Form" }
                    (enum_select("skill", &[
                        ("Novice", "Novice", Skill::Novice == r.skill),
                        ("Intermediate", "Intermediate", Skill::Intermediate == r.skill),
                        ("Master", "Master", Skill::Master == r.skill),
                        ("Expert", "Expert", Skill::Expert == r.skill),
                    ]))
                }
                div {
                    label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Strength" }
                    (enum_select("strength", &[
                        ("Weak", "Weak", Strength::Weak == r.strength),
                        ("Intermediate", "Intermediate", Strength::Intermediate == r.strength),
                        ("Strong", "Strong", Strength::Strong == r.strength),
                        ("VeryStrong", "Very strong", Strength::VeryStrong == r.strength),
                    ]))
                }
                div {
                    label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Side" }
                    (enum_select("side", &[
                        ("Port", "Port", Side::Port == r.side),
                        ("Starboard", "Starboard", Side::Starboard == r.side),
                        ("Either", "Either", Side::Either == r.side),
                    ]))
                }
                div {
                    label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Side strength" }
                    input name="side_strength" type="number" min="0" max="5"
                          value=(r.side_strength)
                          class="w-20 border border-slate-300 rounded px-2 py-1 font-mono text-sm focus:border-slate-500 focus:outline-none";
                }
                div {
                    label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Cox" }
                    (checkbox("can_cox", "can cox", r.can_cox.as_bool()))
                    (checkbox("is_designated_cox", "designated", r.is_designated_cox.as_bool()))
                }
                div {
                    label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Scull" }
                    (checkbox("can_scull", "can scull", r.can_scull.as_bool()))
                }
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
                (weight_slider("seat_weight", 3))
                button type="submit"
                       class="bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold px-3 py-1.5 rounded" {
                    "Add / update"
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
                (weight_slider("pair_weight", 3))
                button type="submit"
                       class="bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold px-3 py-1.5 rounded" {
                    "Add / update"
                }
            }
        }
    }
}

/// Render a `<select>` with `(db_value, display_label, is_selected)` options.
fn enum_select(name: &str, options: &[(&str, &str, bool)]) -> Markup {
    html! {
        select name=(name)
               class="border border-slate-300 rounded px-2 py-1 text-xs focus:border-slate-500 focus:outline-none" {
            @for (value, label, selected) in options {
                @if *selected {
                    option value=(value) selected { (label) }
                } @else {
                    option value=(value) { (label) }
                }
            }
        }
    }
}

/// Range slider for affinity weights (−5..−1, +1..+5). Shows a
/// descriptive label that updates as the slider moves.
fn weight_slider(id: &str, default: i32) -> Markup {
    let label_fn = format!(
        "function {id}_label(v) {{ \
            var el = document.getElementById('{id}-label'); \
            var m = {{ \
                '-5':'Strongly avoid','-4':'Avoid','-3':'Moderately avoid', \
                '-2':'Slightly avoid','-1':'Weakly avoid', \
                '0':'⚠ Forbidden', \
                '1':'Weakly prefer','2':'Slightly prefer','3':'Moderately prefer', \
                '4':'Prefer','5':'Strongly prefer' \
            }}; \
            var txt = m[String(v)] || v; \
            if (v === '0' || v === 0) {{ \
                el.className = 'text-xs font-bold text-red-700 bg-red-100 px-1.5 py-0.5 rounded'; \
            }} else {{ \
                el.className = 'text-xs font-semibold text-slate-700'; \
            }} \
            return txt; }}"
    );
    let (default_label, default_label_class) = match default {
        -5 => ("Strongly avoid", "text-xs font-semibold text-slate-700"),
        -4 => ("Avoid", "text-xs font-semibold text-slate-700"),
        -3 => ("Moderately avoid", "text-xs font-semibold text-slate-700"),
        -2 => ("Slightly avoid", "text-xs font-semibold text-slate-700"),
        -1 => ("Weakly avoid", "text-xs font-semibold text-slate-700"),
        0 => ("⚠ Forbidden", "text-xs font-bold text-red-700 bg-red-100 px-1.5 py-0.5 rounded"),
        1 => ("Weakly prefer", "text-xs font-semibold text-slate-700"),
        2 => ("Slightly prefer", "text-xs font-semibold text-slate-700"),
        3 => ("Moderately prefer", "text-xs font-semibold text-slate-700"),
        4 => ("Prefer", "text-xs font-semibold text-slate-700"),
        5 => ("Strongly prefer", "text-xs font-semibold text-slate-700"),
        _ => ("?", "text-xs font-semibold text-slate-700"),
    };
    html! {
        div class="flex-1 min-w-[12rem]" {
            div class="flex items-center justify-between mb-1" {
                span class="text-xs text-red-600 font-semibold" { "Avoid" }
                span #(format!("{id}-label")) class=(default_label_class) {
                    (default_label)
                }
                span class="text-xs text-emerald-600 font-semibold" { "Prefer" }
            }
            input id=(id) name="weight" type="range" min="-5" max="5" value=(default)
                  class="w-full accent-blue-600"
                  oninput={(format!("document.getElementById('{id}-label').textContent = {id}_label(this.value)"))};
            script { (maud::PreEscaped(&label_fn)) }
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
