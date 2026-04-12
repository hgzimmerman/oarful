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
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto" {
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
    pub(crate) has_committed: bool,
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
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
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
                div class="flex items-center gap-2" {
                    span class="font-medium text-slate-800" { (row.date) }
                    span class="text-xs text-slate-500" { (weekday) }
                    @if row.has_committed {
                        a href={"/history/" (row.date)}
                          hx-get={"/history/" (row.date)}
                          hx-target="#content"
                          hx-push-url="true"
                          class="text-xs bg-emerald-100 text-emerald-800 px-1.5 py-0.5 rounded-full hover:bg-emerald-200" {
                            "View lineup"
                        }
                    }
                }
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
            option value="Yes" selected[is(AvailabilityStatus::Yes)] { "Yes" }
            option value="No" selected[is(AvailabilityStatus::No)] { "No" }
        }
    }
}
