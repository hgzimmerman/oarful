//! History list + detail templates.

use chrono::NaiveDate;
use lineup_db::{lineup::CommittedLineup, snapshot::DbSnapshot};
use maud::{html, Markup};

use super::layout::{empty_state, page_header};

pub(crate) fn list_content(dates: &[NaiveDate]) -> Markup {
    html! {
        (page_header("Committed practices", Some("Lineups that have been committed to the database.")))
        div class="px-8 py-6 max-w-3xl" {
            @if dates.is_empty() {
                (empty_state("No practices committed yet."))
            } @else {
                div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                    @for date in dates {
                        (row(date))
                    }
                }
            }
        }
    }
}

fn row(date: &NaiveDate) -> Markup {
    let href = format!("/history/{date}");
    html! {
        a href=(href)
          class="flex items-center justify-between px-6 py-4 hover:bg-slate-50 transition cursor-pointer"
          hx-get=(href)
          hx-target="#content"
          hx-push-url="true" {
            div class="font-semibold text-slate-800" { (date) }
            span class="text-slate-400" { "→" }
        }
    }
}

pub(crate) fn detail_content(
    snapshot: &DbSnapshot,
    date: NaiveDate,
    committed: &[CommittedLineup],
) -> Markup {
    html! {
        (page_header(&format!("History · {date}"), None))
        div class="px-8 py-6 max-w-4xl space-y-4" {
            @if committed.is_empty() {
                (empty_state("No lineups committed for this date."))
            } @else {
                @for c in committed {
                    (lineup_block(snapshot, c))
                }
            }
        }
    }
}

fn lineup_block(snapshot: &DbSnapshot, committed: &CommittedLineup) -> Markup {
    let boat_name = snapshot
        .sweep_boats
        .iter()
        .find(|b| b.id == committed.lineup.boat_id)
        .map(|b| b.name.as_str())
        .unwrap_or("<unknown boat>");
    let mut seats = committed.seats.clone();
    seats.sort_by_key(|s| s.seat_position);

    html! {
        div class="bg-white rounded-lg shadow overflow-hidden" {
            div class="bg-slate-100 px-4 py-2 border-b border-slate-200" {
                strong { (boat_name) }
                span class="text-xs text-slate-500 ml-2" {
                    "committed " (committed.lineup.created_at)
                }
            }
            table class="w-full text-sm" {
                tbody {
                    @for seat in &seats {
                        (seat_row(snapshot, seat.seat_position, seat.rower_id))
                    }
                }
            }
        }
    }
}

fn seat_row(
    snapshot: &DbSnapshot,
    seat_position: i32,
    rower_id: lineup_db::rower::types::RowerId,
) -> Markup {
    let label = if seat_position == 0 {
        "cox".to_string()
    } else {
        format!("s{seat_position}")
    };
    let name = snapshot
        .rowers
        .iter()
        .find(|r| r.id == rower_id)
        .map(|r| r.name.as_str())
        .unwrap_or("<unknown>");
    html! {
        tr class="border-b border-slate-100 last:border-0" {
            td class="px-4 py-2 text-slate-500 font-mono text-xs w-12" { (label) }
            td class="px-4 py-2 text-slate-800" { (name) }
        }
    }
}
