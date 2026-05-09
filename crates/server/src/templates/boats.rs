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
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            div class="flex items-center justify-between" {
                div {
                    h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" { "Fleet" }
                    p class="font-mono-stat text-xs tracking-wide mt-1" style="color: var(--muted)" { (subtitle) }
                }
                div class="flex items-center gap-2" {
                    @if can_export {
                        a href="/boats/export.csv"
                          class="btn-warm-ghost text-xs py-2" {
                            "Fleet CSV"
                        }
                        a href="/boats/usage-matrix.csv"
                          class="btn-warm-ghost text-xs py-2" {
                            "Usage CSV"
                        }
                    }
                    a href="/boats/new"
                      hx-get="/boats/new"
                      hx-target="#admin-fleet-content"
                      hx-push-url="true"
                      class="btn-warm-ink text-sm py-2 px-4" {
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
    let th_class =
        "px-4 py-2.5 text-left font-mono-stat text-[10px] tracking-widest uppercase font-semibold";
    html! {
        div class="max-w-5xl mx-auto" {
            h2 class="font-serif-heading text-lg font-medium tracking-tight mb-2" style="color: var(--ink)" { (heading) }
            div class="rounded-lg overflow-hidden" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                table class="w-full text-sm" {
                    caption class="sr-only" { "Boat fleet" }
                    thead {
                        tr style="background: var(--paper-2)" {
                            th scope="col" class=(th_class) style="color: var(--ink-2)" { "Name" }
                            th scope="col" class=(th_class) style="color: var(--ink-2)" { "Type" }
                            th scope="col" class=(th_class) style="color: var(--ink-2)" { "Weight" }
                            th scope="col" class=(th_class) style="color: var(--ink-2)" { "Rig" }
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

/// Shorthand boat type notation (8+, 4x, 2−) for the badge.
fn display_type(b: &Boat) -> &'static str {
    match (
        b.seat_count.as_int(),
        b.has_cox.as_bool(),
        b.oars_per_seat.as_int(),
    ) {
        (1, false, 2) => "1x",
        (2, false, 2) => "2x",
        (2, false, 1) => "2−",
        (4, false, 2) => "4x",
        (4, true, 2) => "4x+",
        (4, false, 1) => "4−",
        (4, true, 1) => "4+",
        (8, true, 1) => "8+",
        (8, false, 1) => "8−",
        _ => "?",
    }
}

/// Full descriptive name for the tooltip.
fn display_type_long(b: &Boat) -> &'static str {
    match (
        b.seat_count.as_int(),
        b.has_cox.as_bool(),
        b.oars_per_seat.as_int(),
    ) {
        (1, false, 2) => "Single scull",
        (2, false, 2) => "Double scull",
        (2, false, 1) => "Pair",
        (4, false, 2) => "Quad scull",
        (4, true, 2) => "Coxed quad scull",
        (4, false, 1) => "Coxless four",
        (4, true, 1) => "Coxed four",
        (8, true, 1) => "Eight",
        (8, false, 1) => "Coxless eight",
        _ => "Unknown",
    }
}

fn boat_row(b: &Boat) -> Markup {
    let short = display_type(b);
    let long = display_type_long(b);
    let rig = if b.oars_per_seat.as_int() == 1 {
        format!("{} rigged", b.stroke_side)
    } else {
        "sculling".into()
    };
    let href = format!("/boats/{}", b.id);
    html! {
        tr style="border-top: 1px solid var(--rule-2)" class="hover:bg-paper-2" {
            td class="px-4 py-2.5" {
                a href=(href)
                  hx-get=(href)
                  hx-target="#admin-fleet-content"
                  hx-push-url="true"
                  class="font-serif-heading font-medium text-[15px] tracking-tight hover:underline" style="color: var(--link)" {
                    (b.name)
                }
            }
            td class="px-4 py-2.5" {
                span class="stat-badge text-[10px] stat-tier-2 cursor-help" title=(long) { (short) }
            }
            td class="px-4 py-2.5" {
                span class="font-mono-stat text-xs" style="color: var(--ink-2)" { (b.weight_class) }
            }
            td class="px-4 py-2.5" {
                span class="font-mono-stat text-[11px]" style="color: var(--muted)" { (rig) }
            }
        }
    }
}

// =====================================================================
// Detail page
// =====================================================================

pub(crate) fn detail_content(boat: &Boat, usage: &BoatUsageSummary, can_edit: bool) -> Markup {
    let short = display_type(boat);
    let long = display_type_long(boat);
    let rig = if boat.oars_per_seat.as_int() == 1 {
        format!("{} rigged", boat.stroke_side)
    } else {
        "sculling".into()
    };

    html! {
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            div class="flex items-center gap-3 mb-1" {
                a href="/admin/fleet"
                  onclick="if (history.length > 1) { history.back(); return false; }"
                  class="font-mono-stat text-xs tracking-wider hover:underline" style="color: var(--muted)" {
                    "← Fleet"
                }
            }
            div class="flex items-center justify-between" {
                div {
                    h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" { (boat.name) }
                    div class="flex items-center gap-2 mt-1" {
                        span class="stat-badge text-[10px] stat-tier-2 cursor-help" title=(long) { (short) }
                        span class="font-mono-stat text-xs" style="color: var(--muted)" {
                            (rig) " · " (boat.weight_class)
                        }
                    }
                }
                @if can_edit {
                    a href=(format!("/boats/{}/edit", boat.id))
                      hx-get=(format!("/boats/{}/edit", boat.id))
                      hx-target="#admin-fleet-content"
                      hx-push-url="true"
                      class="btn-warm-ghost text-xs py-2" {
                        "Edit"
                    }
                }
            }
        }

        div class="px-4 sm:px-8 py-6 space-y-6 max-w-3xl mx-auto" {
            // Boat info
            div class="rounded-lg p-6" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                h2 class="font-serif-heading text-lg font-medium tracking-tight mb-4" style="color: var(--ink)" { "Details" }

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
            div class="rounded-lg p-6" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                h2 class="font-serif-heading text-lg font-medium tracking-tight mb-4" style="color: var(--ink)" { "Usage" }

                @if usage.total_uses == 0 {
                    p class="font-mono-stat text-xs italic" style="color: var(--muted)" {
                        "No committed lineups found for this boat."
                    }
                } @else {
                    div class="flex items-stretch gap-0 mb-4" {
                        div class="flex items-baseline gap-3 pr-6" {
                            span class="cv-stat-num font-serif-heading" { (usage.total_uses) }
                            span class="font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "outings" }
                        }
                        @if let Some(last) = usage.last_used {
                            div class="cv-stat-sep" {}
                            div class="flex items-baseline gap-3 pr-6" {
                                span class="cv-stat-num font-serif-heading" { (last.format("%b %-d")) }
                                span class="font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "last used" }
                            }
                        }
                    }

                    h3 class="font-mono-stat text-[10px] tracking-widest uppercase font-semibold mb-2" style="color: var(--ink-2)" { "Recent outings" }
                    div class="text-sm" style="divide-color: var(--rule-2)" {
                        @for (pid, date) in usage.recent_uses.iter().take(20) {
                            a href=(format!("/practices/{pid}/detail"))
                              hx-get=(format!("/practices/{pid}/detail"))
                              hx-target="#admin-fleet-content"
                              hx-push-url="true"
                              class="block px-2 py-1.5 rounded hover:bg-paper-2 font-serif-heading text-[14px]" style="color: var(--link)" {
                                (date.format("%A, %b %-d, %Y"))
                            }
                        }
                        @if usage.recent_uses.len() > 20 {
                            p class="px-2 py-1.5 font-mono-stat text-xs" style="color: var(--muted)" {
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
            dt class="font-mono-stat text-[9px] tracking-widest uppercase font-semibold" style="color: var(--muted)" { (label) }
            dd class="font-medium mt-0.5" style="color: var(--ink)" { (value) }
        }
    }
}

// =====================================================================
// Shared add / edit form
// =====================================================================

pub(crate) fn form_content(mode: FormMode, data: &BoatFormData, error: Option<&str>) -> Markup {
    let (title, action, submit_label, cancel_href) = match mode {
        FormMode::New => (
            "New shell",
            "/boats".to_string(),
            "Create",
            "/boats".to_string(),
        ),
        FormMode::Edit(id) => (
            "Edit shell",
            format!("/boats/{id}"),
            "Save",
            format!("/boats/{id}"),
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

            // method="post" is the non-JS fallback; hx-post swaps
            // #content directly instead of following the 303 redirect.
            // The route also accepts PUT for API consumers; the HTML
            // form sticks with POST since <form> doesn't support PUT
            // natively and the handler accepts both verbs.
            form method="post" action=(action)
                 hx-post=(action)
                 hx-target="#admin-fleet-content"
                 hx-push-url="/admin/fleet"
                 class="bg-paper rounded-lg shadow-soft p-6 space-y-4" {
                // Name
                (text_field("name", "Name", &data.name, true))

                // Boat type
                div {
                    label for="boat_type" class="block text-sm font-semibold text-ink-2 mb-1" {
                        "Boat type"
                    }
                    select id="boat_type" name="boat_type"
                           class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none" {
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
                    p class="text-xs text-ink-3 mt-1" {
                        "Determines seat count, cox presence, and sweep vs scull."
                    }
                }

                // Weight class + stroke side (side-by-side)
                div class="grid grid-cols-1 sm:grid-cols-2 gap-4"
                     x-data={"{ isSweep: !['Quad','QuadPlus','Double','Single'].includes(document.getElementById('boat_type')?.value || '" (&data.boat_type) "') }"}
                     x-init={"$watch('isSweep', () => {}); document.getElementById('boat_type')?.addEventListener('change', (e) => { isSweep = !['Quad','QuadPlus','Double','Single'].includes(e.target.value) })"} {
                    div {
                        label for="weight_class" class="block text-sm font-semibold text-ink-2 mb-1" {
                            "Weight class"
                        }
                        select id="weight_class" name="weight_class"
                               class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none" {
                            (sel_opt("Light", &data.weight_class))
                            (sel_opt("Medium", &data.weight_class))
                            (sel_opt("Heavy", &data.weight_class))
                            (sel_opt("Tubby", &data.weight_class))
                        }
                    }
                    div x-show="isSweep" x-cloak {
                        label for="stroke_side" class="block text-sm font-semibold text-ink-2 mb-1" {
                            "Stroke side (rig)"
                        }
                        select id="stroke_side" name="stroke_side"
                               class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none" {
                            (sel_opt("Starboard", &data.stroke_side))
                            (sel_opt("Port", &data.stroke_side))
                        }
                        p class="text-xs text-ink-3 mt-1" {
                            "Which side stroke seat rows on. Alternates from there toward bow."
                        }
                    }
                }

                // Cox position — only meaningful for coxed boats.
                div x-data={"{ hasCox: ['Eight','FourPlus','QuadPlus'].includes(document.getElementById('boat_type')?.value || '" (&data.boat_type) "') }"}
                    x-init={"document.getElementById('boat_type')?.addEventListener('change', (e) => { hasCox = ['Eight','FourPlus','QuadPlus'].includes(e.target.value) })"} {
                    div x-show="hasCox" x-cloak {
                        label for="cox_position" class="block text-sm font-semibold text-ink-2 mb-1" {
                            "Cox position"
                        }
                        select id="cox_position" name="cox_position"
                               class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none" {
                            (sel_opt("Bow", &data.cox_position))
                            (sel_opt("Stern", &data.cox_position))
                        }
                        p class="text-xs text-ink-3 mt-1" {
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

fn text_field(name: &str, label: &str, value: &str, required: bool) -> Markup {
    html! {
        div {
            label for=(name) class="block text-sm font-semibold text-ink-2 mb-1" { (label) }
            @if required {
                input id=(name) name=(name) type="text" value=(value) required
                      class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
            } @else {
                input id=(name) name=(name) type="text" value=(value)
                      class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
            }
        }
    }
}

fn date_field(name: &str, label: &str, value: &str) -> Markup {
    html! {
        div {
            label for=(name) class="block text-sm font-semibold text-ink-2 mb-1" { (label) }
            input id=(name) name=(name) type="date" value=(value)
                  class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
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
