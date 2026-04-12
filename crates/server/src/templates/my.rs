//! Self-service templates for authenticated rowers.

use chrono::NaiveDate;
use lineup_db::app_user::AppUser;
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

// =====================================================================
// Email preferences
// =====================================================================

pub(crate) fn email_prefs_content(user: &AppUser) -> Markup {
    html! {
        (page_header("Email preferences", Some(&user.name)))
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
            div class="bg-white rounded-lg shadow p-6" {
                form method="post" action="/my/email-preferences"
                     hx-post="/my/email-preferences"
                     hx-target="#content"
                     hx-push-url="true"
                     class="space-y-4" {
                    p class="text-sm text-slate-600 mb-4" {
                        "Choose which emails you'd like to receive from your coach."
                    }
                    div class="flex items-center gap-3" {
                        input type="checkbox" id="opt_in_reminders" name="opt_in_reminders"
                              value="1" checked[user.wants_reminders()]
                              class="rounded border-slate-300 text-slate-800 focus:ring-slate-500";
                        label for="opt_in_reminders" class="text-sm font-medium text-slate-700" {
                            "Availability reminders"
                        }
                    }
                    p class="text-xs text-slate-500 ml-6 -mt-2" {
                        "Receive an email when you haven't responded to upcoming practices."
                    }
                    div class="flex items-center gap-3" {
                        input type="checkbox" id="opt_in_lineups" name="opt_in_lineups"
                              value="1" checked[user.wants_lineups()]
                              class="rounded border-slate-300 text-slate-800 focus:ring-slate-500";
                        label for="opt_in_lineups" class="text-sm font-medium text-slate-700" {
                            "Lineup notifications"
                        }
                    }
                    p class="text-xs text-slate-500 ml-6 -mt-2" {
                        "Receive an email when lineups are posted for an upcoming practice."
                    }
                    div class="pt-2" {
                        button type="submit"
                               class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                            "Save preferences"
                        }
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
