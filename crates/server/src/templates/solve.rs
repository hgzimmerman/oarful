//! Solve view template. Renders the solver's primary lineup, its
//! alternatives (Alpine-toggled), the unplaced-rowers breakdown, and
//! the commit button.

use chrono::NaiveDate;
use lineup_db::{
    rower::{types::RowerId, Rower},
    snapshot::DbSnapshot,
};
use lineup_solver::{ProposedLineup, ProposedSolution, SolveResult, SolveStatus, UnplacedRowers};
use maud::{html, Markup};

use super::layout::page_header;

pub fn view_content(
    snapshot: &DbSnapshot,
    date: NaiveDate,
    result: &SolveResult,
) -> Markup {
    let available = snapshot.available_rowers().count();
    let subtitle = format!(
        "{available} rowers available · {boats} candidate boats",
        boats = snapshot.sweep_boats.len(),
    );

    html! {
        (page_header(&format!("Solve · {date}"), Some(&subtitle)))
        div class="px-8 py-6 space-y-6 max-w-6xl" {
            (status_banner(date, &result.status))

            @if result.status == SolveStatus::Satisfied {
                (primary_panel(snapshot, date, &result.primary))

                @if !result.alternatives.is_empty() {
                    (alternatives_panel(snapshot, &result.alternatives))
                }
            }
        }
    }
}

fn status_banner(date: NaiveDate, status: &SolveStatus) -> Markup {
    match status {
        SolveStatus::Satisfied => html! {
            div class="bg-emerald-50 border-l-4 border-emerald-500 px-4 py-3 rounded text-sm text-emerald-900" {
                "Solver satisfied — review the proposed lineup and commit when ready."
            }
        },
        SolveStatus::Unsatisfiable => html! {
            div class="bg-red-50 border-l-4 border-red-500 px-4 py-3 rounded text-sm text-red-900" {
                strong { "Unsatisfiable." }
                " No seat assignment exists under the current constraints for " (date)
                ". Check the roster, availability, and hard locks."
            }
        },
        SolveStatus::Timeout => html! {
            div class="bg-amber-50 border-l-4 border-amber-500 px-4 py-3 rounded text-sm text-amber-900" {
                strong { "Timeout." }
                " Solver did not finish within its time budget. Try again or increase the budget."
            }
        },
    }
}

fn primary_panel(
    snapshot: &DbSnapshot,
    date: NaiveDate,
    primary: &ProposedSolution,
) -> Markup {
    let used: Vec<&ProposedLineup> = primary.lineups.iter().filter(|l| l.used).collect();
    let skipped: Vec<&ProposedLineup> =
        primary.lineups.iter().filter(|l| !l.used).collect();

    html! {
        section class="bg-white rounded-lg shadow p-6" {
            div class="flex items-center justify-between mb-4" {
                h2 class="text-xl font-bold text-slate-800" { "Primary lineup" }
                form method="post" action={"/commit/" (date)} {
                    button type="submit"
                           class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition" {
                        "Commit primary"
                    }
                }
            }

            @if used.is_empty() {
                div class="text-slate-500 italic" { "No boats fielded." }
            } @else {
                div class="grid grid-cols-1 md:grid-cols-2 gap-4" {
                    @for lineup in &used {
                        (boat_card(snapshot, lineup))
                    }
                }
            }

            @if !skipped.is_empty() {
                div class="mt-4 text-sm text-slate-500" {
                    "Skipped: "
                    @for (i, lineup) in skipped.iter().enumerate() {
                        @if i > 0 { ", " }
                        (lineup.boat_name)
                    }
                }
            }

            (unplaced_block(snapshot, &primary.unplaced))
        }
    }
}

fn alternatives_panel(
    snapshot: &DbSnapshot,
    alternatives: &[ProposedSolution],
) -> Markup {
    html! {
        section class="bg-white rounded-lg shadow p-6"
                x-data="{ open: false }" {
            button type="button"
                   "@click"="open = !open"
                   class="flex items-center space-x-2 text-slate-700 hover:text-slate-900 font-semibold" {
                span x-text="open ? '▼' : '▶'" {}
                span {
                    "Show "
                    (alternatives.len())
                    " alternative"
                    @if alternatives.len() != 1 { "s" }
                }
            }

            div x-show="open" class="mt-4 space-y-6" {
                @for (idx, alt) in alternatives.iter().enumerate() {
                    (alternative_block(snapshot, idx + 2, alt))
                }
            }
        }
    }
}

fn alternative_block(
    snapshot: &DbSnapshot,
    rank: usize,
    alt: &ProposedSolution,
) -> Markup {
    let used: Vec<&ProposedLineup> = alt.lineups.iter().filter(|l| l.used).collect();
    html! {
        div class="border border-slate-200 rounded-lg p-4" {
            h3 class="font-bold text-slate-700 mb-3" { "Alternative #" (rank) }
            @if used.is_empty() {
                div class="text-slate-500 italic" { "No boats fielded." }
            } @else {
                div class="grid grid-cols-1 md:grid-cols-2 gap-4" {
                    @for lineup in &used {
                        (boat_card(snapshot, lineup))
                    }
                }
            }
            (unplaced_block(snapshot, &alt.unplaced))
        }
    }
}

fn boat_card(snapshot: &DbSnapshot, lineup: &ProposedLineup) -> Markup {
    let seat_count = snapshot
        .sweep_boats
        .iter()
        .find(|b| b.id == lineup.boat_id)
        .map(|b| b.seat_count)
        .unwrap_or(0);
    // Sort seats so the cox row (seat 0) sits at the top, then
    // bow→stroke.
    let mut seats = lineup.seats.clone();
    seats.sort_by_key(|(s, _)| *s);

    html! {
        div class="border border-slate-200 rounded-lg overflow-hidden" {
            div class="bg-slate-100 px-4 py-2 border-b border-slate-200" {
                strong class="text-slate-800" { (lineup.boat_name) }
                span class="text-xs text-slate-500 ml-2" { "(" (seat_count) "+)" }
            }
            table class="w-full text-sm" {
                tbody {
                    @for (seat, rower_id) in &seats {
                        (seat_row(snapshot, *seat, *rower_id))
                    }
                }
            }
        }
    }
}

fn seat_row(snapshot: &DbSnapshot, seat: i32, rower_id: RowerId) -> Markup {
    let label = if seat == 0 {
        "cox".to_string()
    } else {
        format!("s{seat}")
    };
    let rower = find_rower(snapshot, rower_id);
    html! {
        tr class="border-b border-slate-100 last:border-0" {
            td class="px-4 py-2 text-slate-500 font-mono text-xs w-12" { (label) }
            td class="px-4 py-2" {
                @if let Some(r) = rower {
                    div class="font-medium text-slate-800" { (r.name) }
                    div class="text-xs text-slate-500" {
                        (r.weight_class) " · " (r.skill) " · " (r.strength) " · " (r.side)
                    }
                } @else {
                    span class="text-slate-400 italic" { "unknown rower #" (rower_id) }
                }
            }
        }
    }
}

fn unplaced_block(snapshot: &DbSnapshot, unplaced: &UnplacedRowers) -> Markup {
    if unplaced.to_sculling.is_empty() && unplaced.benched.is_empty() {
        return html! {};
    }
    html! {
        div class="mt-4 pt-4 border-t border-slate-200 text-sm space-y-2" {
            @if !unplaced.to_sculling.is_empty() {
                div {
                    strong class="text-slate-700" { "To sculling: " }
                    span class="text-slate-600" {
                        (name_list(snapshot, &unplaced.to_sculling))
                    }
                }
            }
            @if !unplaced.benched.is_empty() {
                div {
                    strong class="text-slate-700" { "Benched: " }
                    span class="text-slate-600" {
                        (name_list(snapshot, &unplaced.benched))
                    }
                }
            }
        }
    }
}

fn name_list(snapshot: &DbSnapshot, ids: &[RowerId]) -> Markup {
    html! {
        @for (i, id) in ids.iter().enumerate() {
            @if i > 0 { ", " }
            @if let Some(r) = find_rower(snapshot, *id) {
                (r.name)
            } @else {
                "#" (id)
            }
        }
    }
}

fn find_rower(snapshot: &DbSnapshot, id: RowerId) -> Option<&Rower> {
    snapshot.rowers.iter().find(|r| r.id == id)
}
