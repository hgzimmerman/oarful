//! Self-service templates for authenticated rowers.

use chrono::NaiveDate;
use lineup_db::app_user::AppUser;
use lineup_db::availability::types::AvailabilityStatus;
use lineup_db::practice::PracticeId;
use maud::{html, Markup};

use super::layout::{empty_state, tab_swap, tabbed_section, TabDef};

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
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" {
                "My"
            }
        }
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
    rows: &[AvailabilityRow],
    stale_warning: Option<(PracticeId, NaiveDate)>,
) -> Markup {
    let th_class =
        "px-4 py-2.5 text-left font-mono-stat text-[10px] tracking-widest uppercase font-semibold";
    html! {
        div class="space-y-6" {
            @if let Some((pid, date)) = stale_warning {
                div class="rounded px-4 py-3 text-sm" style="background: color-mix(in oklch, var(--warn) 10%, var(--paper)); border-left: 4px solid var(--warn); color: var(--ink)" {
                    strong { "Heads up: " }
                    "Your change affects a "
                    a href={"/history/" (pid)}
                      hx-get={"/history/" (pid)}
                      hx-target="#content"
                      hx-push-url="true"
                      class="font-semibold underline" style="color: var(--warn)" {
                        "committed lineup for " (date)
                    }
                    ". The coach may need to adjust it."
                }
            }
            // Upcoming practice dates with inline status dropdowns
            @if rows.is_empty() {
                div class="font-mono-stat text-xs italic" style="color: var(--muted)" {
                    "No upcoming practices scheduled."
                }
            } @else {
                div class="rounded-lg overflow-hidden" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                    table class="w-full text-sm" {
                        caption class="sr-only" { "Upcoming availability" }
                        thead {
                            tr style="background: var(--paper-2)" {
                                th scope="col" class=(th_class) style="color: var(--ink-2)" { "Date" }
                                th scope="col" class=(th_class) style="color: var(--ink-2)" { "Status" }
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
        tr style="border-top: 1px solid var(--rule-2)" class="hover:bg-paper-2" {
            td class="px-4 py-2.5" {
                div class="flex items-center gap-2" {
                    span class="font-serif-heading font-medium text-[15px]" style="color: var(--ink)" { (row.date) }
                    span class="font-mono-stat text-[11px]" style="color: var(--muted)" { (weekday) }
                    @if row.has_committed {
                        a href={"/history/" (row.practice_id)}
                          hx-get={"/history/" (row.practice_id)}
                          hx-target="#content"
                          hx-push-url="true"
                          class="stat-badge text-[9px] cursor-pointer" style="color: var(--good); background: color-mix(in oklch, var(--good) 10%, var(--paper)); border-color: color-mix(in oklch, var(--good) 22%, var(--rule))" {
                            "View lineup"
                        }
                    }
                }
            }
            td class="px-4 py-2.5" {
                form method="post" action="/my/availability"
                     hx-post="/my/availability"
                     hx-target="#content"
                     class="flex items-center gap-2" {
                    input type="hidden" name="practice_id" value=(row.practice_id);
                    (status_select(&format!("status-{}", row.date), row.status))
                    button type="submit"
                           class="font-mono-stat text-[10px] tracking-wide uppercase font-semibold hover:underline" style="color: var(--muted)" {
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
            div class="rounded-lg p-6" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                form method="post" action="/my/email-preferences"
                     hx-post="/my/email-preferences"
                     hx-target="#content"
                     hx-push-url="true"
                     class="space-y-4" {
                    p class="text-sm mb-4" style="color: var(--muted)" {
                        "Choose which emails you'd like to receive from your coach."
                    }
                    div class="flex items-center gap-3" {
                        input type="checkbox" id="opt_in_reminders" name="opt_in_reminders"
                              value="1" checked[user.wants_reminders()]
                              class="rounded border-rule text-ink focus:ring-ink-3";
                        label for="opt_in_reminders" class="text-sm font-medium" style="color: var(--ink)" {
                            "Availability reminders"
                        }
                    }
                    p class="text-xs ml-6 -mt-2" style="color: var(--muted)" {
                        "Receive an email when you haven't responded to upcoming practices."
                    }
                    div class="flex items-center gap-3" {
                        input type="checkbox" id="opt_in_lineups" name="opt_in_lineups"
                              value="1" checked[user.wants_lineups()]
                              class="rounded border-rule text-ink focus:ring-ink-3";
                        label for="opt_in_lineups" class="text-sm font-medium" style="color: var(--ink)" {
                            "Lineup notifications"
                        }
                    }
                    p class="text-xs ml-6 -mt-2" style="color: var(--muted)" {
                        "Receive an email when lineups are posted for an upcoming practice."
                    }
                    div class="flex items-center gap-3" {
                        input type="checkbox" id="opt_in_stale_alerts" name="opt_in_stale_alerts"
                              value="1" checked[user.wants_stale_alerts()]
                              class="rounded border-rule text-ink focus:ring-ink-3";
                        label for="opt_in_stale_alerts" class="text-sm font-medium" style="color: var(--ink)" {
                            "Lineup change alerts"
                        }
                    }
                    p class="text-xs ml-6 -mt-2" style="color: var(--muted)" {
                        "Receive an email when rower availability changes affect a committed lineup."
                    }
                    div class="pt-2" {
                        button type="submit" class="btn-warm-ink py-2 px-5 text-sm" {
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
               class="border border-rule rounded px-2 py-1 text-sm focus:border-ink-3 focus:outline-none" {
            @if current.is_none() {
                option value="" disabled selected { "— no response —" }
            }
            option value="Yes" selected[is(AvailabilityStatus::Yes)] { "Yes" }
            option value="No" selected[is(AvailabilityStatus::No)] { "No" }
        }
    }
}
