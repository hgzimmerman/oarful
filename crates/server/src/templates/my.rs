//! Self-service templates for authenticated rowers.

use chrono::NaiveDate;
use lineup_db::availability::types::AvailabilityStatus;
use lineup_db::rower::Rower;
use maud::{html, Markup};

use super::layout::{empty_state, page_header};

/// Shown when the authenticated user has no linked rower record.
pub(crate) fn no_rower_content(title: &str, message: &str) -> Markup {
    html! {
        (page_header(title, None))
        div class="px-8 py-6 max-w-3xl" {
            (empty_state(message))
        }
    }
}

/// A date row for the availability page: a scheduled practice or
/// a date with existing availability, plus the rower's current
/// response (if any).
pub(crate) struct AvailabilityRow {
    pub(crate) date: NaiveDate,
    pub(crate) status: Option<AvailabilityStatus>,
}

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
                        ("Light", "Lightweight", r.weight_class == lineup_db::rower::types::RowerWeightClass::Light),
                        ("Medium", "Middleweight", r.weight_class == lineup_db::rower::types::RowerWeightClass::Medium),
                        ("Heavy", "Heavyweight", r.weight_class == lineup_db::rower::types::RowerWeightClass::Heavy),
                    ]))
                    (select_field("skill", "Form", &[
                        ("Novice", "Novice", r.skill == lineup_db::rower::types::Skill::Novice),
                        ("Intermediate", "Intermediate", r.skill == lineup_db::rower::types::Skill::Intermediate),
                        ("Master", "Master", r.skill == lineup_db::rower::types::Skill::Master),
                        ("Expert", "Expert", r.skill == lineup_db::rower::types::Skill::Expert),
                    ]))
                    (select_field("strength", "Strength", &[
                        ("Weak", "Weak", r.strength == lineup_db::rower::types::Strength::Weak),
                        ("Intermediate", "Intermediate", r.strength == lineup_db::rower::types::Strength::Intermediate),
                        ("Strong", "Strong", r.strength == lineup_db::rower::types::Strength::Strong),
                        ("VeryStrong", "Very strong", r.strength == lineup_db::rower::types::Strength::VeryStrong),
                    ]))
                    (select_field("side", "Preferred side", &[
                        ("Port", "Port", r.side == lineup_db::rower::types::Side::Port),
                        ("Starboard", "Starboard", r.side == lineup_db::rower::types::Side::Starboard),
                        ("Either", "Either", r.side == lineup_db::rower::types::Side::Either),
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

fn select_field(name: &str, label: &str, options: &[(&str, &str, bool)]) -> Markup {
    html! {
        div {
            label for=(name) class="block text-sm font-semibold text-slate-700 mb-1" { (label) }
            select id=(name) name=(name)
                   class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none" {
                @for (value, display, selected) in options {
                    @if *selected {
                        option value=(value) selected { (display) }
                    } @else {
                        option value=(value) { (display) }
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
    rows: &[AvailabilityRow],
) -> Markup {
    html! {
        (page_header("My availability", Some(&rower.name)))
        div class="px-8 py-6 max-w-3xl space-y-6" {
            // Upcoming practice dates with inline status dropdowns
            @if rows.is_empty() {
                div class="text-slate-500 italic" {
                    "No upcoming practices scheduled."
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
                            @for row in rows {
                                (availability_row(row))
                            }
                        }
                    }
                }
            }

        }
    }
}

fn availability_row(row: &AvailabilityRow) -> Markup {
    let weekday = row.date.format("%A").to_string();
    html! {
        tr class="border-t border-slate-100" {
            td class="px-4 py-2" {
                span class="font-medium text-slate-800" { (row.date) }
                span class="text-xs text-slate-500 ml-2" { (weekday) }
            }
            td class="px-4 py-2" {
                form method="post" action="/my/availability"
                     hx-post="/my/availability"
                     hx-target="#content"
                     class="flex items-center gap-2" {
                    input type="hidden" name="date" value=(row.date);
                    (status_select(&format!("status-{}", row.date), row.status))
                    button type="submit"
                           class="text-xs text-slate-500 hover:text-slate-800 font-semibold uppercase tracking-wide" {
                        "Save"
                    }
                }
            }
        }
    }
}

fn status_select(id: &str, current: Option<AvailabilityStatus>) -> Markup {
    let is = |s: AvailabilityStatus| current == Some(s);
    html! {
        select id=(id) name="status"
               class="border border-slate-300 rounded px-2 py-1 text-sm focus:border-slate-500 focus:outline-none" {
            @if current.is_none() {
                option value="" disabled selected { "— no response —" }
            }
            option value="Yes" selected[is(AvailabilityStatus::Yes)] { "Yes — available" }
            option value="No" selected[is(AvailabilityStatus::No)] { "No — not attending" }
            option value="Maybe" selected[is(AvailabilityStatus::Maybe)] { "Maybe — tentative" }
            option value="ScullingOnly" selected[is(AvailabilityStatus::ScullingOnly)] { "Sculling only" }
        }
    }
}
