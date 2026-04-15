//! Boat fleet templates — list view, detail page, + shared add/edit form.

use lineup_db::boat::queries::BoatUsageSummary;
use lineup_db::boat::Boat;
use maud::{html, Markup};

use super::layout::{empty_state, page_header};
use crate::handlers::boats::{BoatFormData, FormMode};

pub(crate) fn list_content(boats: &[Boat], can_export: bool) -> Markup {
    let in_service: Vec<&Boat> = boats.iter().filter(|b| b.in_service()).collect();
    let relinquished: Vec<&Boat> = boats.iter().filter(|b| !b.in_service()).collect();
    let subtitle = format!(
        "{} shells in service · {} relinquished",
        in_service.len(),
        relinquished.len(),
    );

    html! {
        header class="bg-white border-b border-slate-200 px-4 sm:px-8 py-4 sm:py-6" {
            div class="flex items-center justify-between" {
                div {
                    h1 class="text-2xl font-bold text-slate-800" { "Fleet" }
                    p class="text-sm text-slate-500 mt-1" { (subtitle) }
                }
                div class="flex items-center gap-2" {
                    @if can_export {
                        a href="/boats/export.csv"
                          class="text-slate-500 hover:text-slate-800 text-sm font-semibold border border-slate-300 px-4 py-2 rounded transition" {
                            "Export CSV"
                        }
                    }
                    a href="/boats/new"
                      hx-get="/boats/new"
                      hx-target="#content"
                      hx-push-url="true"
                      class="bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold px-4 py-2 rounded shadow transition" {
                        "Add shell"
                    }
                }
            }
        }
        div class="px-4 sm:px-8 py-6 space-y-6" {

            @if boats.is_empty() {
                (empty_state("No shells on file."))
            } @else {
                @if !in_service.is_empty() {
                    (boat_table("In service", &in_service))
                }
                @if !relinquished.is_empty() {
                    (boat_table("Relinquished", &relinquished))
                }
            }
        }
    }
}

fn boat_table(heading: &str, boats: &[&Boat]) -> Markup {
    html! {
        div class="max-w-5xl mx-auto" {
            h2 class="text-lg font-bold text-slate-800 mb-2" { (heading) }
            div class="bg-white rounded-lg shadow overflow-hidden" {
                table class="w-full text-sm" {
                    thead class="bg-slate-100 text-left text-xs uppercase text-slate-600" {
                        tr {
                            th class="px-4 py-2" { "Name" }
                            th class="px-4 py-2" { "Type" }
                            th class="px-4 py-2" { "Weight" }
                            th class="px-4 py-2" { "Seats" }
                            th class="px-4 py-2" { "Rig" }
                        }
                    }
                    tbody {
                        @for b in boats {
                            (boat_row(b))
                        }
                    }
                }
            }
        }
    }
}

fn boat_row(b: &Boat) -> Markup {
    let type_label = crate::handlers::boats::type_label(b);
    let rig = if b.oars_per_seat == 1 {
        format!("{} rigged", b.stroke_side)
    } else {
        "sculling".into()
    };
    let seats = if b.has_cox.as_bool() {
        format!("{}+", b.seat_count)
    } else {
        format!("{}-", b.seat_count)
    };
    let href = format!("/boats/{}", b.id);
    html! {
        tr class="border-t border-slate-100 hover:bg-slate-50" {
            td class="px-4 py-2 font-medium" {
                a href=(href)
                  hx-get=(href)
                  hx-target="#content"
                  hx-push-url="true"
                  class="text-blue-700 hover:text-blue-900 underline" {
                    (b.name)
                }
            }
            td class="px-4 py-2" { (type_label) }
            td class="px-4 py-2" { (b.weight_class) }
            td class="px-4 py-2 font-mono" { (seats) }
            td class="px-4 py-2 text-xs" { (rig) }
        }
    }
}

// =====================================================================
// Detail page
// =====================================================================

pub(crate) fn detail_content(boat: &Boat, usage: &BoatUsageSummary, can_edit: bool) -> Markup {
    let type_label = crate::handlers::boats::type_label(boat);
    let rig = if boat.oars_per_seat == 1 {
        format!("{} rigged", boat.stroke_side)
    } else {
        "sculling".into()
    };
    let seats = if boat.has_cox.as_bool() {
        format!("{}+", boat.seat_count)
    } else {
        format!("{}-", boat.seat_count)
    };

    html! {
        header class="bg-white border-b border-slate-200 px-4 sm:px-8 py-4 sm:py-6" {
            div class="flex items-center gap-3" {
                a href="/admin/fleet"
                  onclick="if (history.length > 1) { history.back(); return false; }"
                  class="text-slate-400 hover:text-slate-700"
                  title="Back" {
                    "←"
                }
                div {
                    h1 class="text-2xl font-bold text-slate-800" { (boat.name) }
                    p class="text-sm text-slate-500 mt-1" {
                        (type_label) " · " (seats) " · " (rig)
                    }
                }
            }
        }

        div class="px-4 sm:px-8 py-6 space-y-6 max-w-3xl mx-auto" {
            // Boat info
            div class="bg-white rounded-lg shadow p-6" {
                div class="flex items-center justify-between mb-4" {
                    h2 class="text-lg font-bold text-slate-800" { "Details" }
                    @if can_edit {
                        a href=(format!("/boats/{}/edit", boat.id))
                          hx-get=(format!("/boats/{}/edit", boat.id))
                          hx-target="#content"
                          hx-push-url="true"
                          class="text-sm font-semibold text-blue-700 hover:text-blue-900" {
                            "Edit"
                        }
                    }
                }

                dl class="grid grid-cols-2 sm:grid-cols-3 gap-y-3 gap-x-4 text-sm" {
                    (detail_item("Weight class", &boat.weight_class.to_string()))
                    (detail_item("Cox position", &boat.cox_position.to_string()))
                    @if let Some(d) = boat.acquired_at {
                        (detail_item("Acquired", &d.format("%Y-%m-%d").to_string()))
                    }
                    @if let Some(d) = boat.manufactured_at {
                        (detail_item("Manufactured", &d.format("%Y-%m-%d").to_string()))
                    }
                    @if let Some(d) = boat.relinquished_at {
                        (detail_item("Relinquished", &d.format("%Y-%m-%d").to_string()))
                    }
                }
            }

            // Usage stats
            div class="bg-white rounded-lg shadow p-6" {
                h2 class="text-lg font-bold text-slate-800 mb-4" { "Usage" }

                @if usage.total_uses == 0 {
                    p class="text-sm text-slate-500 italic" {
                        "No committed lineups found for this boat."
                    }
                } @else {
                    div class="flex gap-8 mb-4" {
                        div {
                            div class="text-3xl font-bold text-slate-800" { (usage.total_uses) }
                            div class="text-xs text-slate-500 uppercase tracking-wide" { "Total outings" }
                        }
                        @if let Some(last) = usage.last_used {
                            div {
                                div class="text-3xl font-bold text-slate-800" { (last.format("%b %-d")) }
                                div class="text-xs text-slate-500 uppercase tracking-wide" { "Last used" }
                            }
                        }
                    }

                    h3 class="text-sm font-semibold text-slate-700 mb-2" { "Recent outings" }
                    div class="divide-y divide-slate-100 text-sm" {
                        @for (pid, date) in usage.recent_uses.iter().take(20) {
                            a href=(format!("/history/{pid}"))
                              hx-get=(format!("/history/{pid}"))
                              hx-target="#content"
                              hx-push-url="true"
                              class="block px-2 py-1.5 hover:bg-slate-50 text-blue-700 hover:text-blue-900" {
                                (date.format("%A, %b %-d, %Y"))
                            }
                        }
                        @if usage.recent_uses.len() > 20 {
                            p class="px-2 py-1.5 text-slate-500 text-xs" {
                                (format!("… and {} more", usage.recent_uses.len() - 20))
                            }
                        }
                    }
                }
            }
        }
    }
}

fn detail_item(label: &str, value: &str) -> Markup {
    html! {
        div {
            dt class="text-xs text-slate-500 uppercase tracking-wide" { (label) }
            dd class="font-medium text-slate-800" { (value) }
        }
    }
}

// =====================================================================
// Shared add / edit form
// =====================================================================

pub(crate) fn form_content(
    mode: FormMode,
    data: &BoatFormData,
    error: Option<&str>,
) -> Markup {
    let (title, action, submit_label, cancel_href) = match mode {
        FormMode::New => ("New shell", "/boats".to_string(), "Create", "/boats".to_string()),
        FormMode::Edit(id) => ("Edit shell", format!("/boats/{id}"), "Save", format!("/boats/{id}")),
    };

    html! {
        (page_header(title, None))
        div class="px-4 sm:px-8 py-6 max-w-2xl mx-auto" {
            @if let Some(msg) = error {
                div class="mb-4 bg-red-50 border-l-4 border-red-500 px-4 py-3 rounded text-sm text-red-900" {
                    strong { "Error. " } (msg)
                }
            }

            // method="post" is the non-JS fallback; hx-post swaps
            // #content directly instead of following the 303 redirect.
            // The route also accepts PUT for API consumers; the HTML
            // form sticks with POST since <form> doesn't support PUT
            // natively and the handler accepts both verbs.
            form method="post" action=(action)
                 hx-post=(action)
                 hx-target="#content"
                 hx-push-url="/admin/fleet"
                 class="bg-white rounded-lg shadow p-6 space-y-4" {
                // Name
                (text_field("name", "Name", &data.name, true))

                // Boat type
                div {
                    label for="boat_type" class="block text-sm font-semibold text-slate-700 mb-1" {
                        "Boat type"
                    }
                    select id="boat_type" name="boat_type"
                           class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none" {
                        (type_option("Eight", "Eight (8+, sweep)", &data.boat_type))
                        (type_option("CoxlessEight", "Coxless Eight (8-, sweep)", &data.boat_type))
                        (type_option("FourPlus", "Coxed Four (4+, sweep)", &data.boat_type))
                        (type_option("Four", "Coxless Four (4-, sweep)", &data.boat_type))
                        (type_option("Pair", "Pair (2-, sweep)", &data.boat_type))
                        (type_option("Quad", "Quad (4x, scull)", &data.boat_type))
                        (type_option("QuadPlus", "Coxed Quad (4x+, scull)", &data.boat_type))
                        (type_option("Double", "Double (2x, scull)", &data.boat_type))
                        (type_option("Single", "Single (1x, scull)", &data.boat_type))
                    }
                    p class="text-xs text-slate-500 mt-1" {
                        "Determines seat count, cox presence, and sweep vs scull."
                    }
                }

                // Weight class + stroke side (side-by-side)
                div class="grid grid-cols-1 sm:grid-cols-2 gap-4"
                     x-data={"{ isSweep: !['Quad','QuadPlus','Double','Single'].includes(document.getElementById('boat_type')?.value || '" (&data.boat_type) "') }"}
                     x-init={"$watch('isSweep', () => {}); document.getElementById('boat_type')?.addEventListener('change', (e) => { isSweep = !['Quad','QuadPlus','Double','Single'].includes(e.target.value) })"} {
                    div {
                        label for="weight_class" class="block text-sm font-semibold text-slate-700 mb-1" {
                            "Weight class"
                        }
                        select id="weight_class" name="weight_class"
                               class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none" {
                            (sel_opt("Light", &data.weight_class))
                            (sel_opt("Medium", &data.weight_class))
                            (sel_opt("Heavy", &data.weight_class))
                            (sel_opt("Tubby", &data.weight_class))
                        }
                    }
                    div x-show="isSweep" x-cloak {
                        label for="stroke_side" class="block text-sm font-semibold text-slate-700 mb-1" {
                            "Stroke side (rig)"
                        }
                        select id="stroke_side" name="stroke_side"
                               class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none" {
                            (sel_opt("Starboard", &data.stroke_side))
                            (sel_opt("Port", &data.stroke_side))
                        }
                        p class="text-xs text-slate-500 mt-1" {
                            "Which side stroke seat rows on. Alternates from there toward bow."
                        }
                    }
                }

                // Cox position — only meaningful for coxed boats.
                div x-data={"{ hasCox: ['Eight','FourPlus','QuadPlus'].includes(document.getElementById('boat_type')?.value || '" (&data.boat_type) "') }"}
                    x-init={"document.getElementById('boat_type')?.addEventListener('change', (e) => { hasCox = ['Eight','FourPlus','QuadPlus'].includes(e.target.value) })"} {
                    div x-show="hasCox" x-cloak {
                        label for="cox_position" class="block text-sm font-semibold text-slate-700 mb-1" {
                            "Cox position"
                        }
                        select id="cox_position" name="cox_position"
                               class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none" {
                            (sel_opt("Bow", &data.cox_position))
                            (sel_opt("Stern", &data.cox_position))
                        }
                        p class="text-xs text-slate-500 mt-1" {
                            "Bow-loader or stern-loader. Eights are always stern."
                        }
                    }
                }

                // Dates
                div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4" {
                    (date_field("acquired_at", "Acquired", &data.acquired_at))
                    (date_field("manufactured_at", "Manufactured", &data.manufactured_at))
                    @if let FormMode::Edit(_) = mode {
                        (date_field("relinquished_at", "Relinquished", &data.relinquished_at))
                    }
                }

                // Actions
                div class="flex items-center space-x-3 pt-2" {
                    button type="submit"
                           class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition" {
                        (submit_label)
                    }
                    a href=(cancel_href)
                      hx-get=(cancel_href)
                      hx-target="#content"
                      hx-push-url="true"
                      class="text-slate-500 hover:text-slate-800 text-sm font-semibold" {
                        "Cancel"
                    }
                }
            }
        }
    }
}

fn text_field(name: &str, label: &str, value: &str, required: bool) -> Markup {
    html! {
        div {
            label for=(name) class="block text-sm font-semibold text-slate-700 mb-1" { (label) }
            @if required {
                input id=(name) name=(name) type="text" value=(value) required
                      class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
            } @else {
                input id=(name) name=(name) type="text" value=(value)
                      class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
            }
        }
    }
}

fn date_field(name: &str, label: &str, value: &str) -> Markup {
    html! {
        div {
            label for=(name) class="block text-sm font-semibold text-slate-700 mb-1" { (label) }
            input id=(name) name=(name) type="date" value=(value)
                  class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
        }
    }
}

fn sel_opt(value: &str, current: &str) -> Markup {
    html! {
        @if value == current {
            option value=(value) selected { (value) }
        } @else {
            option value=(value) { (value) }
        }
    }
}

fn type_option(value: &str, label: &str, current: &str) -> Markup {
    html! {
        @if value == current {
            option value=(value) selected { (label) }
        } @else {
            option value=(value) { (label) }
        }
    }
}
