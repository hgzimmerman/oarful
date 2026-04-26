//! Practices dashboard templates: Planning tab, Committed tab, shared types.
//!
//! Email preview modals live in submodules:
//! - [`reminder_modal`] — reminder preview + confirm
//! - [`lineup_modal`] — lineup preview + confirm

mod lineup_modal;
mod reminder_modal;
mod send_result_modal;

pub(crate) use lineup_modal::{lineup_preview_modal, LineupRecipientPreview};
pub(crate) use reminder_modal::{reminder_preview_modal, ReminderRecipientPreview};
pub(crate) use send_result_modal::{
    send_result_billing_gate, send_result_modal, SendResultRecipient, SendStatus,
};

use chrono::NaiveDate;
use lineup_db::practice::PracticeId;
use maud::{html, Markup};

use super::layout::{empty_state, page_header, tab_swap, tabbed_section, TabDef};

/// Per-row summary used by both the Planning and Committed tabs.
pub(crate) struct PracticeRow {
    pub(crate) practice_id: PracticeId,
    pub(crate) date: NaiveDate,
    pub(crate) time: Option<chrono::NaiveTime>,
    pub(crate) duration_minutes: Option<lineup_db::types::DurationMinutes>,
    pub(crate) yes_count: usize,
    pub(crate) total_responses: usize,
    pub(crate) cancelled: bool,
    pub(crate) non_respondent_count: usize,
    pub(crate) boat_count: usize,
    pub(crate) already_sent_today: bool,
}

const PRACTICES_TABS: &[TabDef] = &[
    TabDef {
        label: "Planning",
        url: "/practices/planning",
        id: "planning",
    },
    TabDef {
        label: "Committed",
        url: "/practices/committed",
        id: "committed",
    },
];
const PRACTICES_TARGET: &str = "practices-tab-content";

/// Full tabbed page wrapper.
pub(crate) fn tabbed_page(active_tab: &str, tab_content: Markup, _is_coach: bool) -> Markup {
    html! {
        (page_header("Practices", None))
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
            (tabbed_section(PRACTICES_TABS, active_tab, PRACTICES_TARGET, tab_content))
        }
    }
}

/// HTMX partial: tab content + OOB tab bar swap.
pub(crate) fn tab_content_swap(active_tab: &str, content: Markup) -> Markup {
    tab_swap(PRACTICES_TABS, active_tab, PRACTICES_TARGET, content)
}

pub(crate) fn planning_content(
    rows: &[PracticeRow],
    is_coach: bool,
    today: chrono::NaiveDate,
    default_time: Option<chrono::NaiveTime>,
    default_duration: Option<lineup_db::types::DurationMinutes>,
    suggested_date: Option<chrono::NaiveDate>,
) -> Markup {
    let min_date = today.format("%Y-%m-%d").to_string();
    let date_value = suggested_date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let time_value = default_time
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default();
    let end_time_value = match (default_time, default_duration) {
        (Some(t), Some(dur)) => {
            let end = t + chrono::TimeDelta::minutes(dur.as_int() as i64);
            end.format("%H:%M").to_string()
        }
        _ => String::new(),
    };

    let total_non_respondents: usize = rows.iter().map(|r| r.non_respondent_count).sum();

    html! {
        @if is_coach {
            form method="post" action="/practices"
                 hx-post="/practices"
                 hx-target="#content"
                 hx-push-url="true"
                 class="flex items-end gap-3 flex-wrap mb-6" {
                div {
                    label for="date" class="block text-xs font-semibold text-ink-2 uppercase tracking-wide mb-1" {
                        "Add practice"
                    }
                    input id="date" name="date" type="date"
                          min=(min_date)
                          value=(date_value)
                          class="border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                }
                div {
                    label for="time" class="block text-xs font-semibold text-ink-2 uppercase tracking-wide mb-1" {
                        "Start"
                    }
                    input id="time" name="time" type="time"
                          value=(time_value)
                          class="border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                }
                div {
                    label for="end_time" class="block text-xs font-semibold text-ink-2 uppercase tracking-wide mb-1" {
                        "End"
                    }
                    input id="end_time" name="end_time" type="time"
                          value=(end_time_value)
                          class="border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                }
                button type="submit"
                       class="bg-ink hover:bg-ink-2 text-paper font-semibold px-4 py-2 rounded shadow transition text-sm" {
                    "Create"
                }
            }
        }

        @if rows.is_empty() {
            (empty_state("No practices awaiting lineups. Create a practice above or check the Committed tab."))
        } @else if is_coach {
            @let initial_checked = rows.iter().filter(|r| !r.cancelled && r.non_respondent_count > 0).count();
            div "x-data"={"{ checked: " (initial_checked) " }"} {
                div class="bg-paper rounded-lg shadow divide-y divide-rule-2" {
                    @for row in rows {
                        (planning_row_card(row, is_coach))
                    }
                }

                @if total_non_respondents > 0 {
                    div class="flex justify-end mt-4"
                        "x-show"="checked > 0"
                        "x-transition" {
                        button type="button"
                               hx-get="/practices/reminder-preview"
                               hx-include="[name='practice_ids']:checked"
                               hx-target="body"
                               hx-swap="beforeend"
                               class="bg-ink hover:bg-ink-2 text-paper font-semibold px-4 py-2 rounded shadow transition text-sm" {
                            "Send reminders"
                        }
                    }
                }
            }
        } @else {
            div class="bg-paper rounded-lg shadow divide-y divide-rule-2" {
                @for row in rows {
                    (planning_row_card(row, is_coach))
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
    let cancel_action = format!("/practices/{}/cancel", row.practice_id);
    let opacity = if row.cancelled { " opacity-40" } else { "" };

    html! {
        div class={"flex items-center px-6 py-3 hover:bg-paper-2 transition" (opacity)} {
            @if is_coach && !row.cancelled && row.non_respondent_count > 0 {
                input type="checkbox" name="practice_ids"
                      value=(row.practice_id)
                      checked
                      "@change"="checked += $el.checked ? 1 : -1"
                      class="rounded border-rule text-ink focus:ring-ink-3 mr-3 shrink-0";
            } @else if is_coach {
                div class="w-4 mr-3 shrink-0" {}
            }
            @if !href.is_empty() {
                a href=(href)
                  class="flex items-center justify-between flex-1 cursor-pointer"
                  hx-get=(href)
                  hx-target="#content"
                  hx-push-url="true" {
                    (planning_row_inner(row, &weekday, is_coach, &cancel_action))
                }
            } @else {
                div class="flex items-center justify-between flex-1" {
                    (planning_row_inner(row, &weekday, is_coach, &cancel_action))
                }
            }
        }
    }
}

fn planning_row_inner(
    row: &PracticeRow,
    weekday: &str,
    is_coach: bool,
    cancel_action: &str,
) -> Markup {
    html! {
        div {
            div class="flex items-center gap-2" {
                @if row.cancelled {
                    span class="font-semibold text-ink line-through" { (row.date) }
                    span class="text-xs bg-red-100 text-red-800 px-1.5 py-0.5 rounded-full" {
                        "Cancelled"
                    }
                } @else {
                    span class="font-semibold text-ink" { (row.date) }
                }
                @if row.already_sent_today {
                    span class="text-xs bg-paper-2 text-ink-3 px-1.5 py-0.5 rounded-full" {
                        "Reminded today"
                    }
                }
            }
            div class="text-sm text-ink-3" {
                (weekday)
                @if let Some(t) = row.time {
                    " \u{00b7} " (t.format("%-I:%M %p"))
                    @if let Some(dur) = row.duration_minutes {
                        @let end = t + chrono::TimeDelta::minutes(dur.as_int() as i64);
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
                        div class="text-xs text-ink-3" {
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
                           class="text-xs text-muted hover:text-red-600 font-medium no-print" {
                        @if row.cancelled { "Restore" } @else { "Cancel" }
                    }
                }
            }
        }
    }
}

pub(crate) fn committed_content(rows: &[PracticeRow], is_coach: bool) -> Markup {
    if rows.is_empty() {
        return html! {
            (empty_state("No committed lineups yet. Solve and commit a lineup from the Planning tab."))
        };
    }

    html! {
        div class="space-y-4" {
            div class="bg-paper rounded-lg shadow divide-y divide-rule-2" {
                @for row in rows {
                    (committed_row(row, is_coach))
                }
            }

            @if is_coach {
                div class="flex justify-end mt-4" {
                    button type="button"
                           hx-get="/practices/lineup-preview"
                           hx-include="[name='dates']:checked"
                           hx-target="body"
                           hx-swap="beforeend"
                           class="bg-ink hover:bg-ink-2 text-paper font-semibold px-4 py-2 rounded shadow transition text-sm" {
                        "Send lineups"
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
        div class="flex items-center justify-between px-6 py-3 hover:bg-paper-2 transition" {
            div class="flex items-center gap-3" {
                @if show_checkbox {
                    input type="checkbox" name="dates" value=(row.date)
                          class="rounded border-rule text-ink focus:ring-ink-3";
                }
                a href=(href)
                  hx-get=(href)
                  hx-target="#content"
                  hx-push-url="true"
                  class="cursor-pointer" {
                    div {
                        span class="font-semibold text-ink" { (row.date) }
                        span class="text-sm text-ink-3 ml-2" { (weekday) }
                        @if let Some(t) = row.time {
                            span class="text-sm text-ink-3 ml-2" {
                                (t.format("%-I:%M %p"))
                                @if let Some(dur) = row.duration_minutes {
                                    @let end = t + chrono::TimeDelta::minutes(dur.as_int() as i64);
                                    "\u{2013}" (end.format("%-I:%M %p"))
                                }
                            }
                        }
                    }
                }
            }
            div class="flex items-center gap-3" {
                span class="text-sm text-ink-2" {
                    (row.boat_count) " boat(s)"
                }
                @if row.already_sent_today {
                    span class="text-xs bg-paper-2 text-ink-3 px-1.5 py-0.5 rounded-full" {
                        "Sent today"
                    }
                }
            }
        }
    }
}
