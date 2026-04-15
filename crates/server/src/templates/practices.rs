//! Practices dashboard template — tabbed view: Schedule, Reminders, Lineups.

use chrono::NaiveDate;
use lineup_db::practice::PracticeId;
use maud::{html, Markup};

use super::layout::{empty_state, page_header};
use crate::handlers::practices::{LineupRow, ReminderRow};

/// Per-row summary for the schedule tab.
pub(crate) struct PracticeRow {
    pub(crate) practice_id: PracticeId,
    pub(crate) date: NaiveDate,
    pub(crate) time: Option<chrono::NaiveTime>,
    pub(crate) duration_minutes: Option<i32>,
    pub(crate) yes_count: usize,
    pub(crate) total_responses: usize,
    pub(crate) has_committed: bool,
    pub(crate) is_upcoming: bool,
    pub(crate) cancelled: bool,
}

/// Full tabbed page wrapper. `active_tab` is "schedule", "reminders",
/// or "lineups". `tab_content` is the pre-rendered content for the
/// active tab.
pub(crate) fn tabbed_page(
    active_tab: &str,
    tab_content: Markup,
    is_coach: bool,
) -> Markup {
    html! {
        (page_header("Practices", None))
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
            // Tab bar
            div class="flex gap-1 border-b border-slate-200 mb-4" {
                (tab_button("Schedule", "/practices/schedule", "schedule", active_tab))
                @if is_coach {
                    (tab_button("Reminders", "/practices/reminders", "reminders", active_tab))
                    (tab_button("Lineups", "/practices/lineups", "lineups", active_tab))
                }
            }
            // Tab content
            div id="practices-tab-content" {
                (tab_content)
            }
        }
    }
}

fn tab_button(label: &str, url: &str, tab_id: &str, active: &str) -> Markup {
    let is_active = tab_id == active;
    let base = "px-4 py-2 text-sm font-medium border-b-2 transition cursor-pointer";
    let classes = if is_active {
        format!("{base} border-slate-800 text-slate-800")
    } else {
        format!("{base} border-transparent text-slate-500 hover:text-slate-700 hover:border-slate-300")
    };
    html! {
        button hx-get=(url)
               hx-target="#practices-tab-content"
               class=(classes) {
            (label)
        }
    }
}

// =====================================================================
// Schedule tab
// =====================================================================

pub(crate) fn schedule_content(
    rows: &[PracticeRow],
    is_coach: bool,
    today: chrono::NaiveDate,
    default_time: Option<chrono::NaiveTime>,
    default_duration: Option<i32>,
) -> Markup {
    let upcoming: Vec<&PracticeRow> = rows.iter().filter(|r| r.is_upcoming).collect();
    let past: Vec<&PracticeRow> = rows.iter().filter(|r| !r.is_upcoming).collect();
    let min_date = today.format("%Y-%m-%d").to_string();
    let time_value = default_time.map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
    let end_time_value = match (default_time, default_duration) {
        (Some(t), Some(dur)) => {
            let end = t + chrono::TimeDelta::minutes(dur as i64);
            end.format("%H:%M").to_string()
        }
        _ => String::new(),
    };

    html! {
        // Add practice form (Coach+ only)
        @if is_coach {
            form method="post" action="/practices"
                 hx-post="/practices"
                 hx-target="#content"
                 hx-push-url="true"
                 class="flex items-end gap-3 flex-wrap mb-6" {
                div {
                    label for="date" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                        "Add practice"
                    }
                    input id="date" name="date" type="date"
                          min=(min_date)
                          class="border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                }
                div {
                    label for="time" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                        "Start"
                    }
                    input id="time" name="time" type="time"
                          value=(time_value)
                          class="border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                }
                div {
                    label for="end_time" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                        "End"
                    }
                    input id="end_time" name="end_time" type="time"
                          value=(end_time_value)
                          class="border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                }
                button type="submit"
                       class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                    "Create"
                }
            }
        }

        // Upcoming
        @if upcoming.is_empty() && past.is_empty() {
            (empty_state("No practices scheduled. Sync the spreadsheet or add a practice date above."))
        }

        @if !upcoming.is_empty() {
            div {
                h2 class="text-sm font-semibold text-slate-600 uppercase tracking-wide mb-2" { "Upcoming" }
                div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                    @for row in &upcoming {
                        (row_card(row, is_coach))
                    }
                }
            }
        }

        // Past (committed only)
        @if !past.is_empty() {
            div {
                h2 class="text-sm font-semibold text-slate-600 uppercase tracking-wide mb-2" { "Past" }
                div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                    @for row in &past {
                        (row_card(row, is_coach))
                    }
                }
            }
        }
    }
}

fn row_card(row: &PracticeRow, is_coach: bool) -> Markup {
    let weekday = row.date.format("%A").to_string();
    let href = if row.has_committed {
        format!("/history/{}", row.practice_id)
    } else if is_coach {
        format!("/solve/{}", row.practice_id)
    } else {
        String::new()
    };
    let clickable = !href.is_empty();

    let base_class = if row.cancelled {
        "flex items-center justify-between px-6 py-3 opacity-40"
    } else if row.is_upcoming {
        "flex items-center justify-between px-6 py-4"
    } else {
        "flex items-center justify-between px-6 py-3 opacity-60"
    };

    html! {
        @if clickable {
            a href=(href)
              class={(base_class) " hover:bg-slate-50 transition cursor-pointer"}
              hx-get=(href)
              hx-target="#content"
              hx-push-url="true" {
                (row_inner(row, &weekday, is_coach))
            }
        } @else {
            div class=(base_class) {
                (row_inner(row, &weekday, is_coach))
            }
        }
    }
}

fn row_inner(row: &PracticeRow, weekday: &str, is_coach: bool) -> Markup {
    let cancel_action = format!("/practices/{}/cancel", row.practice_id);
    html! {
        div {
            div class="flex items-center gap-2" {
                @if row.cancelled {
                    span class="font-semibold text-slate-800 line-through" { (row.date) }
                    span class="text-xs bg-red-100 text-red-800 px-1.5 py-0.5 rounded-full" {
                        "Cancelled"
                    }
                } @else {
                    span class="font-semibold text-slate-800" { (row.date) }
                }
                @if row.has_committed {
                    span class="text-xs bg-emerald-100 text-emerald-800 px-1.5 py-0.5 rounded-full" {
                        "Committed"
                    }
                }
            }
            div class="text-sm text-slate-500" {
                (weekday)
                @if let Some(t) = row.time {
                    " · " (t.format("%-I:%M %p"))
                    @if let Some(dur) = row.duration_minutes {
                        @let end = t + chrono::TimeDelta::minutes(dur as i64);
                        "–" (end.format("%-I:%M %p"))
                    }
                }
            }
        }
        div class="flex items-center gap-3" {
            @if row.is_upcoming && !row.cancelled {
                div class="text-right" {
                    div class="text-lg font-bold text-emerald-700" {
                        (row.yes_count) " available"
                    }
                    @if row.total_responses > 0 {
                        div class="text-xs text-slate-500" {
                            (row.total_responses) " responses"
                        }
                    }
                }
            }
            @if is_coach {
                form method="post" action=(cancel_action)
                     hx-post=(cancel_action)
                     hx-target="#content"
                     onclick="event.stopPropagation(); event.preventDefault(); this.requestSubmit();" {
                    button type="submit"
                           class="text-xs text-slate-400 hover:text-red-600 font-medium no-print" {
                        @if row.cancelled { "Restore" } @else { "Cancel" }
                    }
                }
            }
        }
    }
}

// =====================================================================
// Reminders tab
// =====================================================================

pub(crate) fn reminders_content(rows: &[ReminderRow]) -> Markup {
    if rows.is_empty() {
        return html! {
            (empty_state("No upcoming uncommitted practices to send reminders for."))
        };
    }

    let total_non_respondents: usize = rows.iter().map(|r| r.non_respondent_count).sum();

    html! {
        div class="space-y-4" {
            div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                @for row in rows {
                    div class="flex items-center justify-between px-6 py-3" {
                        div {
                            span class="font-semibold text-slate-800" { (row.date) }
                            span class="text-sm text-slate-500 ml-2" { (row.date.format("%A")) }
                        }
                        div class="flex items-center gap-3" {
                            @if row.non_respondent_count > 0 {
                                span class="text-sm font-medium text-amber-700" {
                                    (row.non_respondent_count) " pending"
                                }
                            } @else {
                                span class="text-sm text-emerald-600" { "All responded" }
                            }
                            @if row.already_sent_today {
                                span class="text-xs bg-slate-100 text-slate-500 px-1.5 py-0.5 rounded-full" {
                                    "Sent today"
                                }
                            }
                        }
                    }
                }
            }

            @if total_non_respondents > 0 {
                form method="post" action="/practices/send-reminders"
                     hx-post="/practices/send-reminders"
                     hx-target="#practices-tab-content"
                     hx-confirm={"Send availability reminders to " (total_non_respondents) " rower(s)?"}
                     class="flex justify-end" {
                    button type="submit"
                           class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                        "Send reminders"
                    }
                }
            }
        }
    }
}

// =====================================================================
// Lineups tab
// =====================================================================

pub(crate) fn lineups_content(rows: &[LineupRow]) -> Markup {
    if rows.is_empty() {
        return html! {
            (empty_state("No upcoming committed lineups to notify about."))
        };
    }

    html! {
        div class="space-y-4" {
            form method="post" action="/practices/send-lineups"
                 hx-post="/practices/send-lineups"
                 hx-target="#practices-tab-content" {

                div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                    @for row in rows {
                        div class="flex items-center justify-between px-6 py-3" {
                            div class="flex items-center gap-3" {
                                input type="checkbox" name="dates" value=(row.date)
                                      class="rounded border-slate-300 text-slate-800 focus:ring-slate-500";
                                div {
                                    span class="font-semibold text-slate-800" { (row.date) }
                                    span class="text-sm text-slate-500 ml-2" { (row.date.format("%A")) }
                                }
                            }
                            div class="flex items-center gap-3" {
                                span class="text-sm text-slate-600" {
                                    (row.boat_count) " boat(s)"
                                }
                                @if row.already_sent_today {
                                    span class="text-xs bg-slate-100 text-slate-500 px-1.5 py-0.5 rounded-full" {
                                        "Sent today"
                                    }
                                }
                            }
                        }
                    }
                }

                div class="flex items-center justify-between mt-4" {
                    // Recipient scope toggle
                    div class="flex items-center gap-4" {
                        label class="flex items-center gap-2 text-sm" {
                            input type="radio" name="scope" value="placed" checked
                                  class="text-slate-800 focus:ring-slate-500";
                            "Placed + bench"
                        }
                        label class="flex items-center gap-2 text-sm" {
                            input type="radio" name="scope" value="all"
                                  class="text-slate-800 focus:ring-slate-500";
                            "All (incl. non-respondents)"
                        }
                    }
                    button type="submit"
                           class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                        "Send lineups"
                    }
                }
            }
        }
    }
}

// =====================================================================
// Shared
// =====================================================================

/// Result message after sending emails.
pub(crate) fn send_result(message: &str) -> Markup {
    html! {
        div class="bg-emerald-50 border border-emerald-200 text-emerald-800 rounded-lg px-6 py-4 text-sm" {
            (message)
        }
    }
}
