//! Practices dashboard template — tabbed view: Planning, Committed.

use chrono::NaiveDate;
use lineup_db::practice::PracticeId;
use maud::{html, Markup};

use maud::PreEscaped;

use super::layout::{empty_state, page_header, TabDef, tab_swap, tabbed_section};

/// Recipient info for the reminder preview modal.
pub(crate) struct ReminderRecipientPreview {
    pub(crate) name: String,
    pub(crate) dates: Vec<NaiveDate>,
}

/// Recipient info for the lineup preview modal.
pub(crate) struct LineupRecipientPreview {
    pub(crate) name: String,
}

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

const PRACTICES_TABS: &[TabDef] = &[
    TabDef { label: "Planning", url: "/practices/planning", id: "planning" },
    TabDef { label: "Committed", url: "/practices/committed", id: "committed" },
];
const PRACTICES_TARGET: &str = "practices-tab-content";

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
            (tabbed_section(PRACTICES_TABS, active_tab, PRACTICES_TARGET, tab_content))
        }
    }
}

/// HTMX partial: tab content + OOB tab bar swap.
pub(crate) fn tab_content_swap(active_tab: &str, content: Markup) -> Markup {
    tab_swap(PRACTICES_TABS, active_tab, PRACTICES_TARGET, content)
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
        } @else if is_coach {
            @let initial_checked = rows.iter().filter(|r| !r.cancelled && r.non_respondent_count > 0).count();
            div "x-data"={"{ checked: " (initial_checked) " }"} {
                div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
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
                               class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                            "Send reminders"
                        }
                    }
                }
            }
        } @else {
            div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
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
        div class={"flex items-center px-6 py-3 hover:bg-slate-50 transition" (opacity)} {
            // Checkbox for reminder selection (Coach+ only, non-cancelled with pending)
            @if is_coach && !row.cancelled && row.non_respondent_count > 0 {
                input type="checkbox" name="practice_ids"
                      value=(row.practice_id)
                      checked
                      "@change"="checked += $el.checked ? 1 : -1"
                      class="rounded border-slate-300 text-slate-800 focus:ring-slate-500 mr-3 shrink-0";
            } @else if is_coach {
                // Spacer to keep alignment when some rows have checkboxes
                div class="w-4 mr-3 shrink-0" {}
            }
            // Clickable row content
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
            div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
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
                           class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
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
// Reminder preview modal
// =====================================================================

const CLOSE_MODAL_JS: &str = "document.getElementById('reminder-modal').remove(); document.getElementById('reminder-modal-backdrop').remove()";

pub(crate) fn reminder_preview_modal(
    recipients: &[ReminderRecipientPreview],
    practice_ids: &[i32],
) -> Markup {
    let unique_count = recipients.len();
    let practice_count: usize = {
        let mut dates: Vec<&NaiveDate> = recipients.iter().flat_map(|r| r.dates.iter()).collect();
        dates.sort();
        dates.dedup();
        dates.len()
    };

    html! {
        // Backdrop
        div id="reminder-modal-backdrop"
            class="fixed inset-0 bg-black/40 z-40"
            onclick=(CLOSE_MODAL_JS) {}
        // Modal
        div id="reminder-modal"
            class="fixed inset-0 z-50 flex items-start justify-center pt-12 px-4 pointer-events-none" {
            div class="bg-white rounded-lg shadow-xl w-full max-w-lg max-h-[80vh] overflow-y-auto pointer-events-auto" {
                // Header
                div class="sticky top-0 bg-white border-b border-slate-200 px-6 py-4 flex items-center justify-between" {
                    h2 class="text-lg font-bold text-slate-800" { "Send reminders" }
                    button type="button"
                           class="text-slate-400 hover:text-slate-600 text-xl leading-none"
                           onclick=(CLOSE_MODAL_JS) {
                        "\u{00d7}"
                    }
                }
                // Body
                div class="px-6 py-4" {
                    @if recipients.is_empty() {
                        p class="text-sm text-slate-500 italic" {
                            "No reminders to send — everyone has responded or reminders were already sent today."
                        }
                    } @else {
                        p class="text-sm text-slate-600 mb-3" {
                            "Will email " strong { (unique_count) }
                            " rower(s) about "
                            strong { (practice_count) }
                            " practice(s):"
                        }
                        div class="space-y-1 mb-4 max-h-60 overflow-y-auto" {
                            @for r in recipients {
                                div class="flex items-center justify-between text-sm py-1" {
                                    span class="text-slate-800" { (r.name) }
                                    span class="text-xs text-slate-400" {
                                        @for (i, d) in r.dates.iter().enumerate() {
                                            @if i > 0 { ", " }
                                            (d.format("%b %-d"))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Footer
                @if !recipients.is_empty() {
                    div class="sticky bottom-0 bg-white border-t border-slate-200 px-6 py-4 flex justify-end" {
                        form method="post" action="/practices/send-reminders"
                             hx-post="/practices/send-reminders"
                             hx-target="#practices-tab-content"
                             onclick=(PreEscaped(CLOSE_MODAL_JS)) {
                            @for id in practice_ids {
                                input type="hidden" name="practice_ids" value=(id);
                            }
                            button type="submit"
                                   class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                                "Send " (unique_count) " reminder(s)"
                            }
                        }
                    }
                }
            }
        }
    }
}

// =====================================================================
// Lineup preview modal
// =====================================================================

const CLOSE_LINEUP_MODAL_JS: &str = "document.getElementById('lineup-modal').remove(); document.getElementById('lineup-modal-backdrop').remove()";

pub(crate) fn lineup_preview_modal(
    recipients: &[LineupRecipientPreview],
    date_strs: &[String],
    scope: &str,
) -> Markup {
    let unique_count = recipients.len();
    let date_count = date_strs.len();

    html! {
        // Backdrop
        div id="lineup-modal-backdrop"
            class="fixed inset-0 bg-black/40 z-40"
            onclick=(CLOSE_LINEUP_MODAL_JS) {}
        // Modal
        div id="lineup-modal"
            class="fixed inset-0 z-50 flex items-start justify-center pt-12 px-4 pointer-events-none" {
            div class="bg-white rounded-lg shadow-xl w-full max-w-lg max-h-[80vh] overflow-y-auto pointer-events-auto" {
                // Header
                div class="sticky top-0 bg-white border-b border-slate-200 px-6 py-4 flex items-center justify-between" {
                    h2 class="text-lg font-bold text-slate-800" { "Send lineups" }
                    button type="button"
                           class="text-slate-400 hover:text-slate-600 text-xl leading-none"
                           onclick=(CLOSE_LINEUP_MODAL_JS) {
                        "\u{00d7}"
                    }
                }
                // Body
                div class="px-6 py-4" {
                    @if date_strs.is_empty() {
                        p class="text-sm text-slate-500 italic" {
                            "No practices selected — check at least one to send lineups."
                        }
                    } @else if recipients.is_empty() {
                        p class="text-sm text-slate-500 italic" {
                            "No recipients — lineups may have already been sent today, or no rowers have accounts with lineup notifications enabled."
                        }
                    } @else {
                        p class="text-sm text-slate-600 mb-3" {
                            "Will email " strong { (unique_count) }
                            " rower(s) about "
                            strong { (date_count) }
                            " lineup(s):"
                        }
                        div class="space-y-1 mb-4 max-h-60 overflow-y-auto" {
                            @for r in recipients {
                                div class="text-sm py-1 text-slate-800" { (r.name) }
                            }
                        }
                    }
                }
                // Footer with scope + confirm
                @if !date_strs.is_empty() && !recipients.is_empty() {
                    div class="sticky bottom-0 bg-white border-t border-slate-200 px-6 py-4" {
                        form method="post" action="/practices/send-lineups"
                             hx-post="/practices/send-lineups"
                             hx-target="#practices-tab-content"
                             onclick=(PreEscaped(CLOSE_LINEUP_MODAL_JS)) {
                            @for d in date_strs {
                                input type="hidden" name="dates" value=(d);
                            }

                            // Scope radios
                            div class="flex items-center gap-4 mb-3" {
                                label class="flex items-center gap-2 text-sm cursor-pointer" {
                                    input type="radio" name="scope" value="placed"
                                          checked[scope == "placed"]
                                          class="text-slate-800 focus:ring-slate-500";
                                    "Placed + bench"
                                }
                                label class="flex items-center gap-2 text-sm cursor-pointer" {
                                    input type="radio" name="scope" value="all"
                                          checked[scope == "all"]
                                          class="text-slate-800 focus:ring-slate-500";
                                    "All (incl. non-respondents)"
                                }
                            }

                            div class="flex justify-end" {
                                button type="submit"
                                       class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                                    "Send " (unique_count) " lineup(s)"
                                }
                            }
                        }
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

/// Warning/error message (amber instead of green).
pub(crate) fn send_warning(message: &str) -> Markup {
    html! {
        div class="bg-amber-50 border border-amber-200 text-amber-800 rounded-lg px-6 py-4 text-sm" {
            (message)
        }
    }
}
