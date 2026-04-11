//! Practices dashboard template — unified upcoming + past view.

use chrono::NaiveDate;
use maud::{html, Markup};

use super::layout::{empty_state, page_header};

/// Per-row summary for the practices page.
pub(crate) struct PracticeRow {
    pub(crate) date: NaiveDate,
    pub(crate) yes_count: usize,
    pub(crate) total_responses: usize,
    pub(crate) has_committed: bool,
    pub(crate) is_upcoming: bool,
}

pub(crate) fn list_content(rows: &[PracticeRow], is_coach: bool) -> Markup {
    let upcoming: Vec<&PracticeRow> = rows.iter().filter(|r| r.is_upcoming).collect();
    let past: Vec<&PracticeRow> = rows.iter().filter(|r| !r.is_upcoming).collect();

    html! {
        (page_header("Practices", None))
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
            // Add practice form (Coach+ only)
            @if is_coach {
                form method="post" action="/practices"
                     hx-post="/practices"
                     hx-target="#content"
                     hx-push-url="true"
                     class="flex items-end gap-3" {
                    div {
                        label for="date" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                            "Add practice"
                        }
                        input id="date" name="date" type="date"
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
}

fn row_card(row: &PracticeRow, is_coach: bool) -> Markup {
    let weekday = row.date.format("%A").to_string();
    // Upcoming → solve view (Coach) or just info (Member).
    // Past committed → history detail for everyone.
    let href = if row.has_committed {
        format!("/history/{}", row.date)
    } else if is_coach {
        format!("/solve/{}", row.date)
    } else {
        String::new()
    };
    let clickable = !href.is_empty();

    let base_class = if row.is_upcoming {
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
                (row_inner(row, &weekday))
            }
        } @else {
            div class=(base_class) {
                (row_inner(row, &weekday))
            }
        }
    }
}

fn row_inner(row: &PracticeRow, weekday: &str) -> Markup {
    html! {
        div {
            div class="flex items-center gap-2" {
                span class="font-semibold text-slate-800" { (row.date) }
                @if row.has_committed {
                    span class="text-xs bg-emerald-100 text-emerald-800 px-1.5 py-0.5 rounded-full" {
                        "Committed"
                    }
                }
            }
            div class="text-sm text-slate-500" { (weekday) }
        }
        @if row.is_upcoming {
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
    }
}
