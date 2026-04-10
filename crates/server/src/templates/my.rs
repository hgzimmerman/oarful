//! Self-service templates for authenticated rowers.

use lineup_db::availability::Availability;
use lineup_db::rower::Rower;
use maud::{html, Markup};

use super::layout::page_header;

// =====================================================================
// Profile
// =====================================================================

pub(crate) fn profile_content(r: &Rower) -> Markup {
    profile_inner(r, None)
}

pub(crate) fn profile_content_with_error(r: &Rower, error: &str) -> Markup {
    profile_inner(r, Some(error))
}

fn profile_inner(r: &Rower, error: Option<&str>) -> Markup {
    html! {
        (page_header("My profile", Some(&r.name)))
        div class="px-8 py-6 max-w-2xl" {
            @if let Some(msg) = error {
                div class="mb-4 bg-red-50 border-l-4 border-red-500 px-4 py-3 rounded text-sm text-red-900" {
                    (msg)
                }
            }

            form method="post" action="/my/profile"
                 hx-post="/my/profile"
                 hx-target="#content"
                 class="bg-white rounded-lg shadow p-6 space-y-4" {
                div class="grid grid-cols-2 gap-4" {
                    (select_field("weight_class", "Weight class", &[
                        ("Light", r.weight_class == lineup_db::rower::types::RowerWeightClass::Light),
                        ("Medium", r.weight_class == lineup_db::rower::types::RowerWeightClass::Medium),
                        ("Heavy", r.weight_class == lineup_db::rower::types::RowerWeightClass::Heavy),
                    ]))
                    (select_field("skill", "Skill", &[
                        ("Novice", r.skill == lineup_db::rower::types::Skill::Novice),
                        ("Intermediate", r.skill == lineup_db::rower::types::Skill::Intermediate),
                        ("Master", r.skill == lineup_db::rower::types::Skill::Master),
                        ("Expert", r.skill == lineup_db::rower::types::Skill::Expert),
                    ]))
                    (select_field("strength", "Strength", &[
                        ("Weak", r.strength == lineup_db::rower::types::Strength::Weak),
                        ("Intermediate", r.strength == lineup_db::rower::types::Strength::Intermediate),
                        ("Strong", r.strength == lineup_db::rower::types::Strength::Strong),
                        ("VeryStrong", r.strength == lineup_db::rower::types::Strength::VeryStrong),
                    ]))
                    (select_field("side", "Preferred side", &[
                        ("Port", r.side == lineup_db::rower::types::Side::Port),
                        ("Starboard", r.side == lineup_db::rower::types::Side::Starboard),
                        ("Either", r.side == lineup_db::rower::types::Side::Either),
                    ]))
                    div {
                        label for="side_strength" class="block text-sm font-semibold text-slate-700 mb-1" { "Side strength (0=hard lock, 5=flexible)" }
                        input id="side_strength" name="side_strength" type="number"
                              min="0" max="5" value=(r.side_strength)
                              class="w-full border border-slate-300 rounded px-3 py-2 text-sm font-mono focus:border-slate-500 focus:outline-none";
                    }
                    div class="flex items-center pt-6" {
                        label class="flex items-center space-x-2 text-sm text-slate-700" {
                            @if r.can_scull.as_bool() {
                                input type="checkbox" name="can_scull" checked;
                            } @else {
                                input type="checkbox" name="can_scull";
                            }
                            span { "Can scull" }
                        }
                    }
                }
                button type="submit"
                       class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition" {
                    "Save"
                }
            }
        }
    }
}

fn select_field(name: &str, label: &str, options: &[(&str, bool)]) -> Markup {
    html! {
        div {
            label for=(name) class="block text-sm font-semibold text-slate-700 mb-1" { (label) }
            select id=(name) name=(name)
                   class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none" {
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
}

// =====================================================================
// Availability
// =====================================================================

pub(crate) fn availability_content(
    rower: &Rower,
    entries: &[Availability],
) -> Markup {
    html! {
        (page_header("My availability", Some(&rower.name)))
        div class="px-8 py-6 max-w-3xl space-y-6" {
            // Existing entries
            @if entries.is_empty() {
                div class="text-slate-500 italic" {
                    "No upcoming availability on file."
                }
            } @else {
                div class="bg-white rounded-lg shadow overflow-hidden" {
                    table class="w-full text-sm" {
                        thead class="bg-slate-100 text-left text-xs uppercase text-slate-600" {
                            tr {
                                th class="px-4 py-2" { "Date" }
                                th class="px-4 py-2" { "Status" }
                            }
                        }
                        tbody {
                            @for entry in entries {
                                (availability_row(entry))
                            }
                        }
                    }
                }
            }

            // Add / update form
            section class="bg-white rounded-lg shadow p-6" {
                h2 class="text-lg font-bold text-slate-800 mb-3" { "Set availability" }
                form method="post" action="/my/availability"
                     hx-post="/my/availability"
                     hx-target="#content"
                     class="flex items-end space-x-3" {
                    div {
                        label for="date" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Date" }
                        input id="date" name="date" type="date" required
                              class="border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                    }
                    div {
                        label for="status" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" { "Status" }
                        select id="status" name="status"
                               class="border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none" {
                            option value="Yes" { "Yes — available" }
                            option value="No" { "No — not attending" }
                            option value="Maybe" { "Maybe — tentative" }
                            option value="ScullingOnly" { "Sculling only" }
                        }
                    }
                    button type="submit"
                           class="bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold px-4 py-2 rounded shadow transition" {
                        "Save"
                    }
                }
            }
        }
    }
}

fn availability_row(entry: &Availability) -> Markup {
    let weekday = entry.date.format("%A").to_string();
    let badge_class = match entry.status.to_string().as_str() {
        "Yes" => "bg-emerald-100 text-emerald-800",
        "No" => "bg-red-100 text-red-800",
        "Maybe" => "bg-amber-100 text-amber-800",
        "ScullingOnly" => "bg-blue-100 text-blue-800",
        _ => "bg-slate-100 text-slate-600",
    };
    html! {
        tr class="border-t border-slate-100" {
            td class="px-4 py-2" {
                span class="font-medium text-slate-800" { (entry.date) }
                span class="text-xs text-slate-500 ml-2" { (weekday) }
            }
            td class="px-4 py-2" {
                span class={"text-xs px-2 py-0.5 rounded-full " (badge_class)} {
                    (entry.status)
                }
            }
        }
    }
}
