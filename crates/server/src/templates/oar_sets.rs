//! Oar set templates — list view, detail page, picker modal, + shared add/edit form.

use std::collections::HashMap;

use lineup_db::boat::types::BoatId;
use lineup_db::boat::Boat;
use lineup_db::oar_set::types::OarSetId;
use lineup_db::oar_set::{OarSet, OarSetPreference};
use lineup_db::practice::PracticeId;
use maud::{html, Markup};

use super::layout::page_header;
use crate::handlers::oar_sets::{FormMode, OarSetFormData};

pub(crate) fn list_content(oar_sets: &[OarSet]) -> Markup {
    let active: Vec<&OarSet> = oar_sets.iter().filter(|o| o.active.as_bool()).collect();
    let inactive: Vec<&OarSet> = oar_sets.iter().filter(|o| !o.active.as_bool()).collect();
    let subtitle = format!("{} active · {} retired", active.len(), inactive.len(),);

    html! {
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            div class="flex items-center justify-between" {
                div {
                    h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" { "Oar sets" }
                    p class="font-mono-stat text-xs tracking-wide mt-1" style="color: var(--muted)" { (subtitle) }
                }
                a href="/oars/new"
                  hx-get="/oars/new"
                  hx-target="#admin-fleet-content"
                  hx-push-url="true"
                  class="btn-warm-ink text-sm py-2 px-4" {
                    "Add oar set"
                }
            }
        }
        div class="px-4 sm:px-8 py-6 space-y-6" {
            @if oar_sets.is_empty() {
                (super::layout::empty_state("No oar sets on file."))
            } @else {
                @if !active.is_empty() {
                    (oar_set_table("Active", &active))
                }
                @if !inactive.is_empty() {
                    (oar_set_table("Retired", &inactive))
                }
            }
        }
    }
}

fn oar_set_table(heading: &str, oar_sets: &[&OarSet]) -> Markup {
    let th_class =
        "px-4 py-2.5 text-left font-mono-stat text-[10px] tracking-widest uppercase font-semibold";
    html! {
        div class="max-w-3xl mx-auto" {
            h2 class="font-serif-heading text-lg font-medium tracking-tight mb-2" style="color: var(--ink)" { (heading) }
            div class="rounded-lg overflow-hidden" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                table class="w-full text-sm" {
                    caption class="sr-only" { "Oar sets" }
                    thead {
                        tr style="background: var(--paper-2)" {
                            th scope="col" class=(th_class) style="color: var(--ink-2)" { "Name" }
                            th scope="col" class=(th_class) style="color: var(--ink-2)" { "Count" }
                            th scope="col" class=(th_class) style="color: var(--ink-2)" { "Notes" }
                        }
                    }
                    tbody {
                        @for os in oar_sets {
                            (oar_set_row(os))
                        }
                    }
                }
            }
        }
    }
}

fn oar_set_row(os: &OarSet) -> Markup {
    let href = format!("/oars/{}", os.id);
    html! {
        tr style="border-top: 1px solid var(--rule-2)" class="hover:bg-paper-2" {
            td class="px-4 py-2.5" {
                a href=(href)
                  hx-get=(href)
                  hx-target="#admin-fleet-content"
                  hx-push-url="true"
                  class="font-serif-heading font-medium text-[15px] tracking-tight hover:underline" style="color: var(--link)" {
                    (os.name)
                }
            }
            td class="px-4 py-2.5" {
                span class="font-mono-stat text-xs" style="color: var(--ink-2)" {
                    (os.oar_count) " oars"
                }
            }
            td class="px-4 py-2.5" {
                @if let Some(notes) = &os.notes {
                    span class="font-mono-stat text-[11px]" style="color: var(--muted)" { (notes) }
                }
            }
        }
    }
}

// =====================================================================
// Detail page
// =====================================================================

pub(crate) fn detail_content(
    oar_set: &OarSet,
    prefs: &[OarSetPreference],
    boats: &[Boat],
) -> Markup {
    html! {
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            div class="flex items-center gap-3 mb-1" {
                a href="/admin/fleet/oars"
                  onclick="if (history.length > 1) { history.back(); return false; }"
                  class="font-mono-stat text-xs tracking-wider hover:underline" style="color: var(--muted)" {
                    "← Oar sets"
                }
            }
            div class="flex items-center justify-between" {
                div {
                    h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" { (&oar_set.name) }
                    div class="flex items-center gap-2 mt-1" {
                        span class="font-mono-stat text-xs" style="color: var(--muted)" {
                            (oar_set.oar_count) " oars"
                            @if !oar_set.active.as_bool() {
                                " · retired"
                            }
                        }
                    }
                }
                div class="flex items-center gap-2" {
                    form method="post" action=(format!("/oars/{}/toggle-active", oar_set.id))
                         hx-post=(format!("/oars/{}/toggle-active", oar_set.id))
                         hx-target="#admin-fleet-content"
                         hx-push-url="/admin/fleet/oars"
                         class="inline" {
                        button type="submit" class="btn-warm-ghost text-xs py-2" {
                            @if oar_set.active.as_bool() { "Retire" } @else { "Reactivate" }
                        }
                    }
                    a href=(format!("/oars/{}/edit", oar_set.id))
                      hx-get=(format!("/oars/{}/edit", oar_set.id))
                      hx-target="#admin-fleet-content"
                      hx-push-url="true"
                      class="btn-warm-ghost text-xs py-2" {
                        "Edit"
                    }
                }
            }
        }

        div class="px-4 sm:px-8 py-6 space-y-6 max-w-3xl mx-auto" {
            // Details card
            div class="rounded-lg p-6" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                h2 class="font-serif-heading text-lg font-medium tracking-tight mb-4" style="color: var(--ink)" { "Details" }
                dl class="grid grid-cols-2 gap-y-3 gap-x-4 text-sm" {
                    (detail_item("Oar count", &oar_set.oar_count.to_string()))
                    @if let Some(notes) = &oar_set.notes {
                        (detail_item("Notes", notes))
                    }
                }
            }

            // Boat preferences card
            div class="rounded-lg p-6" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                h2 class="font-serif-heading text-lg font-medium tracking-tight mb-4" style="color: var(--ink)" { "Boat preferences" }
                p class="text-xs mb-3" style="color: var(--muted)" {
                    "Boats this oar set is preferred for, in priority order. Used to suggest oar assignments in the practice editor."
                }
                (preferences_form(oar_set, prefs, boats))
            }
        }
    }
}

fn detail_item(label: &str, value: &str) -> Markup {
    html! {
        div {
            dt class="font-mono-stat text-[9px] tracking-widest uppercase font-semibold" style="color: var(--muted)" { (label) }
            dd class="font-medium mt-0.5" style="color: var(--ink)" { (value) }
        }
    }
}

fn preferences_form(oar_set: &OarSet, prefs: &[OarSetPreference], boats: &[Boat]) -> Markup {
    // Boats not yet in preferences
    let pref_boat_ids: Vec<_> = prefs.iter().map(|p| p.boat_id).collect();
    let available_boats: Vec<&Boat> = boats
        .iter()
        .filter(|b| !pref_boat_ids.contains(&b.id))
        .collect();

    let action = format!("/oars/{}/preferences", oar_set.id);
    html! {
        form method="post" action=(action)
             hx-post=(action)
             hx-target="#admin-fleet-content"
             class="space-y-3"
             x-data="{ items: [] }"
             x-init={
                 "items = [" (prefs.iter().map(|p| {
                     let boat_name = boats.iter().find(|b| b.id == p.boat_id).map(|b| b.name.as_str()).unwrap_or("?");
                     format!("{{id:'{}',name:'{}'}}", p.boat_id, boat_name)
                 }).collect::<Vec<_>>().join(",")) "]"
             } {

            // Current preferences list
            div class="space-y-1" {
                template x-for="(item, index) in items" ":key"="item.id" {
                    div class="flex items-center gap-2 px-3 py-2 rounded" style="background: var(--paper-2); border: 1px solid var(--rule-2)" {
                        span class="font-mono-stat text-[10px] font-semibold" style="color: var(--muted)" x-text="index + 1" {}
                        span class="flex-1 text-sm font-medium" style="color: var(--ink)" x-text="item.name" {}
                        input type="hidden" name="boat_ids" ":value"="item.id";
                        button type="button" class="text-xs px-1 py-0.5 rounded hover:bg-bad/10" style="color: var(--bad)"
                               "@click"="items.splice(index, 1)" { "×" }
                        @if prefs.len() > 1 {
                            button type="button" class="text-xs px-1 py-0.5 rounded hover:bg-paper" style="color: var(--muted)"
                                   "@click"="if(index>0){[items[index],items[index-1]]=[items[index-1],items[index]]}" "aria-label"="Move up" { "↑" }
                            button type="button" class="text-xs px-1 py-0.5 rounded hover:bg-paper" style="color: var(--muted)"
                                   "@click"="if(index<items.length-1){[items[index],items[index+1]]=[items[index+1],items[index]]}" "aria-label"="Move down" { "↓" }
                        }
                    }
                }
            }

            @if prefs.is_empty() {
                p class="text-xs italic" style="color: var(--muted)" { "No boat preferences set." }
            }

            // Add boat dropdown
            @if !available_boats.is_empty() {
                div class="flex items-center gap-2" {
                    select id="add-pref-boat"
                           class="flex-1 border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none" {
                        option value="" { "Select a boat…" }
                        @for boat in &available_boats {
                            option value=(boat.id) { (&boat.name) }
                        }
                    }
                    button type="button"
                           class="btn-warm-ghost text-xs py-2"
                           "@click"=(maud::PreEscaped("(() => { let sel=document.getElementById('add-pref-boat'); if(sel.value){ items.push({id:sel.value,name:sel.options[sel.selectedIndex].text}); sel.value=''; } })()")) {
                        "Add"
                    }
                }
            }

            button type="submit" class="btn-warm-ink text-sm py-2 px-4 mt-2" { "Save preferences" }
        }
    }
}

// =====================================================================
// Shared add / edit form
// =====================================================================

pub(crate) fn form_content(mode: FormMode, data: &OarSetFormData, error: Option<&str>) -> Markup {
    let (title, action, submit_label, cancel_href) = match mode {
        FormMode::New => (
            "New oar set",
            "/oars".to_string(),
            "Create",
            "/admin/fleet/oars".to_string(),
        ),
        FormMode::Edit(id) => (
            "Edit oar set",
            format!("/oars/{id}"),
            "Save",
            format!("/oars/{id}"),
        ),
    };

    html! {
        (page_header(title, None))
        div class="px-4 sm:px-8 py-6 max-w-2xl mx-auto" {
            @if let Some(msg) = error {
                div class="mb-4 bg-bad/10 border-l-4 border-bad px-4 py-3 rounded text-sm text-ink" {
                    strong { "Error. " } (msg)
                }
            }

            form method="post" action=(action)
                 hx-post=(action)
                 hx-target="#admin-fleet-content"
                 hx-push-url="/admin/fleet/oars"
                 class="bg-paper rounded-lg shadow-soft p-6 space-y-4" {
                // Name
                div {
                    label for="name" class="block text-sm font-semibold text-ink-2 mb-1" { "Name" }
                    input id="name" name="name" type="text" value=(data.name) required
                          placeholder="e.g. Blue, Gold White"
                          class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                }

                // Oar count
                div {
                    label for="oar_count" class="block text-sm font-semibold text-ink-2 mb-1" { "Number of oars" }
                    input id="oar_count" name="oar_count" type="number" min="1" value=(data.oar_count) required
                          class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                    p class="text-xs text-ink-3 mt-1" {
                        "Total oars in this set. An 8-oar set can serve one 8+ or two 4+s."
                    }
                }

                // Notes
                div {
                    label for="notes" class="block text-sm font-semibold text-ink-2 mb-1" { "Notes" }
                    textarea id="notes" name="notes" rows="2"
                             placeholder="e.g. shorter shafts, heavy blades"
                             class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none" {
                        (data.notes)
                    }
                }

                // Actions
                div class="flex items-center space-x-3 pt-2" {
                    button type="submit"
                           class="bg-good hover:opacity-90 text-paper font-semibold px-4 py-2 rounded shadow-soft transition" {
                        (submit_label)
                    }
                    a href=(cancel_href)
                      hx-get=(cancel_href)
                      hx-target="#admin-fleet-content"
                      hx-push-url="true"
                      class="text-ink-3 hover:text-ink text-sm font-semibold" {
                        "Cancel"
                    }
                }
            }
        }
    }
}

// =====================================================================
// Oar picker modal (per-boat, opened from the lineup editor)
// =====================================================================

pub(crate) fn pick_modal(
    practice_id: PracticeId,
    boat: &Boat,
    oar_sets: &[OarSet],
    assignments: &HashMap<BoatId, (OarSetId, String)>,
    boats: &[Boat],
) -> Markup {
    let boat_oars_needed = boat.seat_count.as_int() * boat.oars_per_seat.as_int();
    let current = assignments.get(&boat.id);

    // Compute usage per oar set across all boats (excluding this one).
    let mut usage_by_others: HashMap<OarSetId, (i32, Vec<String>)> = HashMap::new();
    for (bid, (oid, _)) in assignments {
        if *bid == boat.id {
            continue;
        }
        let other_boat = boats.iter().find(|b| b.id == *bid);
        let oars_needed = other_boat
            .map(|b| b.seat_count.as_int() * b.oars_per_seat.as_int())
            .unwrap_or(0);
        let name = other_boat.map(|b| b.name.as_str()).unwrap_or("?");
        let entry = usage_by_others
            .entry(*oid)
            .or_insert_with(|| (0, Vec::new()));
        entry.0 += oars_needed;
        entry.1.push(name.to_string());
    }

    let close_js = "releaseFocus(); document.getElementById('oar-pick-modal').remove(); document.getElementById('oar-pick-backdrop').remove()";

    html! {
        div id="oar-pick-backdrop"
            class="fixed inset-0 bg-black/40 z-40"
            onclick=(close_js) {}
        div id="oar-pick-modal"
            role="dialog"
            "aria-modal"="true"
            class="fixed inset-0 z-50 flex items-start justify-center pt-12 px-4 pointer-events-none" {
            div class="bg-paper rounded-lg shadow-xl w-full max-w-md max-h-[80vh] overflow-y-auto pointer-events-auto" {
                // Header
                div class="sticky top-0 bg-paper border-b border-rule-2 px-6 py-4 flex items-center justify-between" {
                    div {
                        h2 class="text-lg font-bold text-ink" { "Oars for " (&boat.name) }
                        p class="font-mono-stat text-[10px] text-muted mt-0.5" {
                            "Needs " (boat_oars_needed) " oars"
                        }
                    }
                    button onclick=(close_js)
                           class="text-ink-3 hover:text-ink text-xl leading-none" {
                        span "aria-hidden"="true" { "\u{00d7}" }
                    }
                }

                // Body — oar set options
                div class="px-6 py-4 space-y-1" {
                    // "None" option
                    @let none_selected = current.is_none();
                    button type="button"
                           class="w-full text-left px-3 py-2.5 rounded text-sm transition"
                           style=(if none_selected {
                               "background: color-mix(in oklch, var(--accent) 10%, var(--paper)); border: 1px solid var(--accent)"
                           } else {
                               "border: 1px solid var(--rule-2)"
                           })
                           hx-post="/oars/assign"
                           hx-vals=(format!("{{\"practice_id\":\"{}\",\"boat_id\":\"{}\",\"oar_set_id\":\"\"}}", practice_id, boat.id))
                           hx-swap="none"
                           "hx-on::after-request"="oarPickDone()" {
                        span class="font-medium" style="color: var(--ink-2)" { "No oars" }
                    }

                    @for os in oar_sets {
                        @let is_selected = current.map(|(id, _)| *id == os.id).unwrap_or(false);
                        @let (used_by_others_count, other_names) = usage_by_others
                            .get(&os.id)
                            .map(|(u, n)| (*u, n.clone()))
                            .unwrap_or((0, Vec::new()));
                        @let self_usage = if is_selected { boat_oars_needed } else { 0 };
                        @let remaining = os.oar_count - used_by_others_count - self_usage;
                        @let sufficient = (os.oar_count - used_by_others_count) >= boat_oars_needed;

                        @let can_select = sufficient || is_selected;
                        div class="w-full text-left px-3 py-2.5 rounded text-sm"
                            style=(if is_selected {
                                "background: color-mix(in oklch, var(--accent) 10%, var(--paper)); border: 1px solid var(--accent)"
                            } else if !can_select {
                                "border: 1px solid var(--rule-2); opacity: 0.4; cursor: not-allowed"
                            } else {
                                "border: 1px solid var(--rule-2); cursor: pointer"
                            })
                            hx-post=(if can_select { "/oars/assign" } else { "" })
                            hx-vals=(if can_select { format!("{{\"practice_id\":\"{}\",\"boat_id\":\"{}\",\"oar_set_id\":\"{}\"}}", practice_id, boat.id, os.id) } else { String::new() })
                            hx-swap=(if can_select { "none" } else { "" })
                            "hx-on::after-request"=(if can_select { "oarPickDone()" } else { "" }) {
                            div class="flex items-center justify-between" {
                                div {
                                    span class="font-medium" style="color: var(--ink)" { (&os.name) }
                                    @if let Some(notes) = &os.notes {
                                        span class="font-mono-stat text-[10px] ml-2" style="color: var(--muted)" { (notes) }
                                    }
                                }
                                div class="font-mono-stat text-[10px] text-right" {
                                    @if !sufficient {
                                        span style="color: var(--warn)" { (remaining) "/" (os.oar_count) " avail" }
                                    } @else {
                                        span style="color: var(--ink-2)" { (remaining) "/" (os.oar_count) " avail" }
                                    }
                                }
                            }
                            @if !other_names.is_empty() {
                                div class="font-mono-stat text-[9px] mt-0.5" style="color: var(--muted)" {
                                    "used by: " (other_names.join(", "))
                                }
                            }
                            @if !sufficient && remaining > 0 {
                                div class="font-mono-stat text-[9px] mt-0.5" style="color: var(--warn)" {
                                    "not enough \u{2014} need " (boat_oars_needed) ", only " (remaining) " available"
                                }
                            } @else if remaining <= 0 && !is_selected {
                                div class="font-mono-stat text-[9px] mt-0.5" style="color: var(--warn)" {
                                    "fully allocated"
                                }
                            }
                        }
                    }
                }

                // Footer
                div class="sticky bottom-0 bg-paper border-t border-rule-2 px-6 py-3 flex justify-end" {
                    button type="button" class="btn-warm-ink text-sm py-2 px-4"
                           onclick=(close_js) {
                        "Close"
                    }
                }
            }
        }
        script { (maud::PreEscaped("
            trapFocus(document.getElementById('oar-pick-modal'));
            function oarPickDone() {
                releaseFocus();
                var m = document.getElementById('oar-pick-modal');
                var b = document.getElementById('oar-pick-backdrop');
                if (m) m.remove();
                if (b) b.remove();
                var ed = document.getElementById('lineup-editor');
                if (!ed) return;
                var data = Alpine.$data(ed);
                if (data && data.gatherState) {
                    htmx.ajax('GET', ed.dataset.editorUrl + '?' + data.gatherState(), {target: ed, swap: 'outerHTML'});
                }
            }
        ")) }
    }
}
