//! Self-service templates for authenticated rowers.

use chrono::NaiveDate;
use lineup_db::app_user::AppUser;
use lineup_db::availability::types::AvailabilityStatus;
use lineup_db::practice::PracticeId;
use lineup_db::rower::Rower;
use maud::{html, Markup};

use super::layout::{empty_state, page_header, tab_swap, tabbed_section, TabDef};

const MY_TABS: &[TabDef] = &[
    TabDef {
        label: "Profile",
        url: "/my/profile",
        id: "profile",
    },
    TabDef {
        label: "Availability",
        url: "/my/availability",
        id: "availability",
    },
    TabDef {
        label: "Email",
        url: "/my/email-preferences",
        id: "email",
    },
];
const MY_TARGET: &str = "my-tab-content";

/// Full tabbed page wrapper for `/my`.
pub(crate) fn tabbed_page(active_tab: &str, tab_content: Markup) -> Markup {
    html! {
        (page_header("My", None))
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
            (tabbed_section(MY_TABS, active_tab, MY_TARGET, tab_content))
        }
    }
}

/// HTMX partial: tab content + OOB tab bar swap.
pub(crate) fn tab_content_swap(active_tab: &str, content: Markup) -> Markup {
    tab_swap(MY_TABS, active_tab, MY_TARGET, content)
}

/// Shown when the authenticated user has no linked rower record.
pub(crate) fn no_rower_content(message: &str) -> Markup {
    html! {
        (empty_state(message))
    }
}

/// A date row for the availability page: a scheduled practice or
/// a date with existing availability, plus the rower's current
/// response (if any).
pub(crate) struct AvailabilityRow {
    pub(crate) practice_id: PracticeId,
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
    stale_warning: Option<(PracticeId, NaiveDate)>,
) -> Markup {
    html! {
        div class="space-y-6" {
            @if let Some((pid, date)) = stale_warning {
                div class="bg-amber-50 border-l-4 border-amber-500 px-4 py-3 rounded text-sm text-amber-900" {
                    strong { "Heads up: " }
                    "Your change affects a "
                    a href={"/history/" (pid)}
                      hx-get={"/history/" (pid)}
                      hx-target="#content"
                      hx-push-url="true"
                      class="font-semibold underline hover:text-amber-700" {
                        "committed lineup for " (date)
                    }
                    ". The coach may need to adjust it."
                }
            }
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
                        a href={"/history/" (row.practice_id)}
                          hx-get={"/history/" (row.practice_id)}
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
                    input type="hidden" name="practice_id" value=(row.practice_id);
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
        div class="space-y-6" {
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
