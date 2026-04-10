//! Practices dashboard template.

use chrono::NaiveDate;
use maud::{html, Markup};

use super::layout::{empty_state, page_header};

/// Per-row summary for the practices dashboard.
pub(crate) struct PracticeRow {
    pub(crate) date: NaiveDate,
    /// Rowers who said "Yes" — directly usable by the sweep solver.
    pub(crate) yes_count: usize,
    /// Everyone who responded (Yes / No / Maybe / ScullingOnly).
    pub(crate) total_responses: usize,
}

pub(crate) fn list_content(rows: &[PracticeRow]) -> Markup {
    html! {
        (page_header("Upcoming practices", Some("Pick a date to generate a lineup.")))
        div class="px-8 py-6" {
            @if rows.is_empty() {
                (empty_state("No upcoming availability on file. Sync the spreadsheet to populate this view."))
            } @else {
                div class="bg-white rounded-lg shadow divide-y divide-slate-200 max-w-3xl" {
                    @for row in rows {
                        (row_card(row))
                    }
                }
            }
        }
    }
}

fn row_card(row: &PracticeRow) -> Markup {
    let href = format!("/solve/{}", row.date);
    let weekday = row.date.format("%A").to_string();
    html! {
        a href=(href)
          class="flex items-center justify-between px-6 py-4 hover:bg-slate-50 transition cursor-pointer"
          hx-get=(href)
          hx-target="#content"
          hx-push-url="true" {
            div {
                div class="font-semibold text-slate-800" { (row.date) }
                div class="text-sm text-slate-500" { (weekday) }
            }
            div class="text-right" {
                div class="text-lg font-bold text-emerald-700" {
                    (row.yes_count) " available"
                }
                div class="text-xs text-slate-500" {
                    (row.total_responses) " total responses"
                }
            }
        }
    }
}
