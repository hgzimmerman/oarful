//! Attendance grid — horizontal scrollable table with rowers as rows
//! and practice dates as columns. Color-coded: green = present,
//! red = absent, white = no response.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use lineup_db::availability::types::AvailabilityStatus;
use lineup_db::practice::PracticeId;
use lineup_db::rower::Rower;
use lineup_db::rower::types::RowerId;
use maud::{html, Markup};

pub(crate) fn grid_content(
    rowers: &[Rower],
    dates: &[NaiveDate],
    avail_map: &HashMap<(RowerId, NaiveDate), AvailabilityStatus>,
    _committed_dates: &HashSet<NaiveDate>,
    // Maps committed dates to their practice IDs for link generation.
    committed_practice_ids: &HashMap<NaiveDate, PracticeId>,
    show_past: bool,
    today: NaiveDate,
) -> Markup {
    let subtitle = format!(
        "{} members · {} practices{}",
        rowers.len(),
        dates.len(),
        if show_past { " (incl. past year)" } else { "" },
    );

    html! {
        header class="bg-white border-b border-slate-200 px-4 sm:px-8 py-4 sm:py-6" {
            div class="flex items-center justify-between" {
                div {
                    h1 class="text-2xl font-bold text-slate-800" { "Attendance" }
                    p class="text-sm text-slate-500 mt-1" { (subtitle) }
                }
                div {
                    @if show_past {
                        a href="/team/attendance"
                          hx-get="/team/attendance"
                          hx-target="#team-tab-content"
                          hx-push-url="true"
                          class="text-sm font-semibold text-slate-600 border border-slate-300 px-3 py-1.5 rounded transition hover:bg-slate-50" {
                            "Future only"
                        }
                    } @else {
                        a href="/team/attendance?show_past=1"
                          hx-get="/team/attendance?show_past=1"
                          hx-target="#team-tab-content"
                          hx-push-url="true"
                          class="text-sm font-semibold text-slate-600 border border-slate-300 px-3 py-1.5 rounded transition hover:bg-slate-50" {
                            "Show past year"
                        }
                    }
                }
            }
        }

        div class="px-4 sm:px-8 py-6" {
            @if dates.is_empty() {
                div class="text-center text-slate-500 italic py-12" {
                    "No practices scheduled."
                }
            } @else if rowers.is_empty() {
                div class="text-center text-slate-500 italic py-12" {
                    "No roster members."
                }
            } @else {
                // max-h + overflow-y lets the header stick within the scroll container
                div class="overflow-auto bg-white rounded-lg shadow max-h-[75vh]" {
                    table class="text-xs border-collapse" {
                        thead {
                            tr {
                                // top-left corner: sticky in both directions
                                th class="sticky top-0 left-0 z-20 bg-slate-100 px-3 py-2 text-left font-semibold text-slate-700 border-b border-r border-slate-200 min-w-[140px]" {
                                    "Rower"
                                }
                                @for date in dates {
                                    (date_header(date, today, committed_practice_ids.get(date)))
                                }
                            }
                        }
                        tbody {
                            @for rower in rowers {
                                tr {
                                    td class="sticky left-0 z-10 bg-white px-3 py-1.5 font-medium text-slate-800 border-b border-r border-slate-200 whitespace-nowrap" {
                                        (rower.name)
                                    }
                                    @for date in dates {
                                        (status_cell(avail_map.get(&(rower.id, *date))))
                                    }
                                }
                            }
                        }
                    }
                }

                // Legend
                div class="flex items-center gap-4 mt-3 text-xs text-slate-500" {
                    span class="flex items-center gap-1" {
                        span class="inline-block w-3 h-3 rounded-sm bg-emerald-400" {}
                        "Present"
                    }
                    span class="flex items-center gap-1" {
                        span class="inline-block w-3 h-3 rounded-sm bg-red-400" {}
                        "Absent"
                    }
                    span class="flex items-center gap-1" {
                        span class="inline-block w-3 h-3 rounded-sm border border-slate-200" {}
                        "No response"
                    }
                }
            }
        }
    }
}

fn date_header(date: &NaiveDate, today: NaiveDate, practice_id: Option<&PracticeId>) -> Markup {
    let is_today = *date == today;
    let bg = if is_today { "bg-blue-50" } else { "bg-slate-100" };
    let base_class = format!("sticky top-0 z-10 {bg} px-1.5 py-2 text-center font-medium border-b border-slate-200 whitespace-nowrap min-w-[44px]");
    let full_date = date.format("%A, %B %-d, %Y").to_string();

    html! {
        th class=(base_class) title=(full_date) {
            @if let Some(pid) = practice_id {
                a href=(format!("/history/{pid}"))
                  hx-get=(format!("/history/{pid}"))
                  hx-target="#content"
                  hx-push-url="true"
                  class="text-blue-700 hover:text-blue-900" {
                    div class="text-[10px] uppercase" { (date.format("%a")) }
                    div { (date.format("%b")) }
                    div class="text-sm font-bold" { (date.format("%-d")) }
                }
            } @else {
                div class="text-[10px] text-slate-400 uppercase" { (date.format("%a")) }
                div class="text-slate-600" { (date.format("%b")) }
                div class="text-sm font-bold text-slate-600" { (date.format("%-d")) }
            }
        }
    }
}

fn status_cell(status: Option<&AvailabilityStatus>) -> Markup {
    let (bg, title) = match status {
        Some(AvailabilityStatus::Yes) => ("bg-emerald-400", "Present"),
        Some(AvailabilityStatus::No) => ("bg-red-400", "Absent"),
        None => ("", "No response"),
    };
    html! {
        td class=(format!("{bg} border-b border-r border-slate-100 min-w-[44px] h-7"))
           title=(title) {}
    }
}
