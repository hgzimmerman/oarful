//! Practices dashboard template — tabbed view: Planning, Committed.

use chrono::NaiveDate;
use lineup_db::practice::PracticeId;
use maud::{html, Markup};

use super::layout::{empty_state, page_header};

/// Per-row summary used by both the Planning and Committed tabs.
pub(crate) struct PracticeRow {
    pub(crate) practice_id: PracticeId,
    pub(crate) date: NaiveDate,
    pub(crate) time: Option<chrono::NaiveTime>,
    pub(crate) duration_minutes: Option<i32>,
    pub(crate) yes_count: usize,
    pub(crate) total_responses: usize,
    pub(crate) cancelled: bool,
    pub(crate) non_respondent_count: usize,
    pub(crate) boat_count: usize,
    pub(crate) already_sent_today: bool,
}

/// Full tabbed page wrapper. `active_tab` is "planning" or "committed".
/// `tab_content` is the pre-rendered content for the active tab.
pub(crate) fn tabbed_page(
    active_tab: &str,
    tab_content: Markup,
    _is_coach: bool,
) -> Markup {
    html! {
        (page_header("Practices", None))
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
            // Tab bar — both tabs visible to all roles
            div class="flex gap-1 border-b border-slate-200 mb-4" {
                (tab_button("Planning", "/practices/planning", "planning", active_tab))
                (tab_button("Committed", "/practices/committed", "committed", active_tab))
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
// Planning tab
// =====================================================================

pub(crate) fn planning_content(
    rows: &[PracticeRow],
    is_coach: bool,
    today: chrono::NaiveDate,
    default_time: Option<chrono::NaiveTime>,
    default_duration: Option<i32>,
    suggested_date: Option<chrono::NaiveDate>,
) -> Markup {
    let min_date = today.format("%Y-%m-%d").to_string();
    let date_value = suggested_date.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
    let time_value = default_time.map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
    let end_time_value = match (default_time, default_duration) {
        (Some(t), Some(dur)) => {
            let end = t + chrono::TimeDelta::minutes(dur as i64);
            end.format("%H:%M").to_string()
        }
        _ => String::new(),
    };

    let total_non_respondents: usize = rows.iter().map(|r| r.non_respondent_count).sum();

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
                          value=(date_value)
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

        @if rows.is_empty() {
            (empty_state("No practices awaiting lineups. Create a practice above or check the Committed tab."))
        } @else {
            div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                @for row in rows {
                    (planning_row_card(row, is_coach))
                }
            }

            // Send reminders button (Coach+ only)
            @if is_coach && total_non_respondents > 0 {
                form method="post" action="/practices/send-reminders"
                     hx-post="/practices/send-reminders"
                     hx-target="#practices-tab-content"
                     hx-confirm={"Send availability reminders to " (total_non_respondents) " rower(s)?"}
                     class="flex justify-end mt-4" {
                    button type="submit"
                           class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                        "Send reminders"
                    }
                }
            }
        }
    }
}

fn planning_row_card(row: &PracticeRow, is_coach: bool) -> Markup {
    let weekday = row.date.format("%A").to_string();
    let href = if is_coach {
        format!("/solve/{}", row.practice_id)
    } else {
        String::new()
    };
    let clickable = !href.is_empty();
    let cancel_action = format!("/practices/{}/cancel", row.practice_id);

    let base_class = if row.cancelled {
        "flex items-center justify-between px-6 py-3 opacity-40"
    } else {
        "flex items-center justify-between px-6 py-4"
    };

    html! {
        @if clickable {
            a href=(href)
              class={(base_class) " hover:bg-slate-50 transition cursor-pointer"}
              hx-get=(href)
              hx-target="#content"
              hx-push-url="true" {
                (planning_row_inner(row, &weekday, is_coach, &cancel_action))
            }
        } @else {
            div class=(base_class) {
                (planning_row_inner(row, &weekday, is_coach, &cancel_action))
            }
        }
    }
}

fn planning_row_inner(row: &PracticeRow, weekday: &str, is_coach: bool, cancel_action: &str) -> Markup {
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
                @if row.already_sent_today {
                    span class="text-xs bg-slate-100 text-slate-500 px-1.5 py-0.5 rounded-full" {
                        "Reminded today"
                    }
                }
            }
            div class="text-sm text-slate-500" {
                (weekday)
                @if let Some(t) = row.time {
                    " \u{00b7} " (t.format("%-I:%M %p"))
                    @if let Some(dur) = row.duration_minutes {
                        @let end = t + chrono::TimeDelta::minutes(dur as i64);
                        "\u{2013}" (end.format("%-I:%M %p"))
                    }
                }
            }
        }
        div class="flex items-center gap-3" {
            @if !row.cancelled {
                div class="text-right" {
                    div class="text-lg font-bold text-emerald-700" {
                        (row.yes_count) " available"
                    }
                    @if row.total_responses > 0 {
                        div class="text-xs text-slate-500" {
                            (row.total_responses) " responses"
                        }
                    }
                    @if row.non_respondent_count > 0 {
                        div class="text-xs text-amber-700" {
                            (row.non_respondent_count) " pending"
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
// Committed tab
// =====================================================================

pub(crate) fn committed_content(rows: &[PracticeRow], is_coach: bool) -> Markup {
    if rows.is_empty() {
        return html! {
            (empty_state("No committed lineups yet. Solve and commit a lineup from the Planning tab."))
        };
    }

    html! {
        div class="space-y-4" {
            @if is_coach {
                form method="post" action="/practices/send-lineups"
                     hx-post="/practices/send-lineups"
                     hx-target="#practices-tab-content" {

                    div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                        @for row in rows {
                            (committed_row(row, true))
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
            } @else {
                div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                    @for row in rows {
                        (committed_row(row, false))
                    }
                }
            }
        }
    }
}

fn committed_row(row: &PracticeRow, show_checkbox: bool) -> Markup {
    let weekday = row.date.format("%A").to_string();
    let href = format!("/history/{}", row.practice_id);

    html! {
        div class="flex items-center justify-between px-6 py-3 hover:bg-slate-50 transition" {
            div class="flex items-center gap-3" {
                @if show_checkbox {
                    input type="checkbox" name="dates" value=(row.date)
                          class="rounded border-slate-300 text-slate-800 focus:ring-slate-500";
                }
                a href=(href)
                  hx-get=(href)
                  hx-target="#content"
                  hx-push-url="true"
                  class="cursor-pointer" {
                    div {
                        span class="font-semibold text-slate-800" { (row.date) }
                        span class="text-sm text-slate-500 ml-2" { (weekday) }
                        @if let Some(t) = row.time {
                            span class="text-sm text-slate-500 ml-2" {
                                (t.format("%-I:%M %p"))
                                @if let Some(dur) = row.duration_minutes {
                                    @let end = t + chrono::TimeDelta::minutes(dur as i64);
                                    "\u{2013}" (end.format("%-I:%M %p"))
                                }
                            }
                        }
                    }
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
