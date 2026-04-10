//! Solve view template. Renders the solver's primary lineup, its
//! alternatives (Alpine-toggled), the unplaced-rowers breakdown, and
//! the commit button.

use std::collections::HashMap;

use chrono::NaiveDate;
use lineup_db::{
    boat::types::BoatId,
    rower::{types::RowerId, Rower},
    snapshot::DbSnapshot,
};
use lineup_solver::{
    Diagnostic, ProposedLineup, ProposedSolution, SolveResult, SolveStatus, UnplacedRowers,
};
use maud::{html, Markup};

use super::layout::page_header;
use crate::handlers::solve::SolveKnobs;

pub(crate) fn view_content(
    snapshot: &DbSnapshot,
    date: NaiveDate,
    knobs: &SolveKnobs,
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
            (knobs_form(date, knobs))
            (status_banner(date, &result.status, &result.diagnostics))

            @if result.status == SolveStatus::Satisfied {
                (primary_panel(snapshot, date, knobs, &result.primary))

                @if !result.alternatives.is_empty() {
                    (alternatives_panel(snapshot, &result.primary, &result.alternatives))
                }
            }
        }
    }
}

/// Coach-tunable knobs (partial fill / novelty / alternatives / time
/// budget). Submitting hx-gets the same `/solve/{date}` URL with the
/// new query string, so the result is bookmarkable and the back
/// button works.
fn knobs_form(date: NaiveDate, knobs: &SolveKnobs) -> Markup {
    let action = format!("/solve/{date}");
    html! {
        section class="bg-white rounded-lg shadow p-6" {
            form method="get" action=(action)
                 hx-get=(action)
                 hx-target="#content"
                 hx-push-url="true"
                 hx-indicator="#solve-spinner"
                 class="grid grid-cols-2 md:grid-cols-5 gap-4 items-end" {
                (knob_input(
                    "partial",
                    "Partial fill",
                    knobs.partial as i64,
                    Some(0),
                    Some("0 = strict; N = up to N optional seats empty per boat"),
                ))
                (knob_input(
                    "novelty",
                    "Novelty",
                    knobs.novelty as i64,
                    Some(0),
                    Some("S7 weight; 0 disables"),
                ))
                (knob_input(
                    "alts",
                    "Alternatives",
                    knobs.alts as i64,
                    Some(1),
                    Some("Distinct lineups (incl. primary)"),
                ))
                (knob_input(
                    "budget",
                    "Time budget (s)",
                    knobs.budget as i64,
                    Some(1),
                    Some("Per-solve cap; clamped ≥ 1s"),
                ))
                div class="flex items-center space-x-3" {
                    button type="submit"
                           class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition" {
                        "Re-solve"
                    }
                    span #solve-spinner class="htmx-indicator text-xs text-slate-500" {
                        "Solving…"
                    }
                }
            }
        }
    }
}

fn knob_input(
    name: &str,
    label: &str,
    value: i64,
    min: Option<i64>,
    help: Option<&str>,
) -> Markup {
    html! {
        div {
            label for=(name) class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                (label)
            }
            input id=(name) name=(name) type="number"
                  value=(value)
                  min=[min.map(|m| m.to_string())]
                  class="w-full border border-slate-300 rounded px-3 py-2 font-mono text-sm focus:border-slate-500 focus:outline-none";
            @if let Some(h) = help {
                p class="text-xs text-slate-500 mt-1" { (h) }
            }
        }
    }
}

fn status_banner(date: NaiveDate, status: &SolveStatus, diagnostics: &[Diagnostic]) -> Markup {
    match status {
        SolveStatus::Satisfied => html! {
            div class="bg-emerald-50 border-l-4 border-emerald-500 px-4 py-3 rounded text-sm text-emerald-900" {
                "Solver satisfied — review the proposed lineup and commit when ready."
            }
        },
        SolveStatus::Unsatisfiable => html! {
            div class="bg-red-50 border-l-4 border-red-500 px-4 py-3 rounded text-sm text-red-900" {
                strong { "Unsatisfiable." }
                " No seat assignment exists under the current constraints for " (date) "."
                @if diagnostics.is_empty() {
                    " Check the roster, availability, and hard locks."
                } @else {
                    ul class="mt-2 ml-4 list-disc space-y-1" {
                        @for d in diagnostics {
                            li { (diagnostic_message(d)) }
                        }
                    }
                }
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

fn diagnostic_message(d: &Diagnostic) -> String {
    match d {
        Diagnostic::NoCoxForBoat { boat_name } => {
            format!("{boat_name} needs a cox but no available rower can cox.")
        }
        Diagnostic::NotEnoughRowers {
            available,
            smallest_boat_seats,
            smallest_boat_name,
        } => {
            format!(
                "Only {available} rowers available, but even the smallest boat \
                 ({smallest_boat_name}) needs {smallest_boat_seats} seats filled."
            )
        }
        Diagnostic::UnfillableSeat { boat_name, seat } => {
            format!(
                "Seat {seat} on {boat_name} has no eligible rower \
                 (check side preferences and roster)."
            )
        }
        Diagnostic::AllBoatsUnfillable => {
            "Every candidate boat has at least one seat that can't be filled — \
             no fleet combination is possible."
                .to_string()
        }
    }
}

fn primary_panel(
    snapshot: &DbSnapshot,
    date: NaiveDate,
    knobs: &SolveKnobs,
    primary: &ProposedSolution,
) -> Markup {
    let used: Vec<&ProposedLineup> = primary.lineups.iter().filter(|l| l.used).collect();
    let skipped: Vec<&ProposedLineup> =
        primary.lineups.iter().filter(|l| !l.used).collect();

    html! {
        section class="bg-white rounded-lg shadow p-6" {
            div class="flex items-center justify-between mb-4" {
                h2 class="text-xl font-bold text-slate-800" { "Primary lineup" }
                // Hidden inputs carry the same knobs the view used so
                // commit re-solves with identical params instead of
                // silently reverting to defaults.
                form method="post" action={"/commit/" (date)} {
                    input type="hidden" name="partial" value=(knobs.partial);
                    input type="hidden" name="novelty" value=(knobs.novelty);
                    input type="hidden" name="alts" value=(knobs.alts);
                    input type="hidden" name="budget" value=(knobs.budget);
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
                        (boat_card(snapshot, lineup, None))
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
    primary: &ProposedSolution,
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
                    (alternative_block(snapshot, primary, idx + 2, alt))
                }
            }
        }
    }
}

fn alternative_block(
    snapshot: &DbSnapshot,
    primary: &ProposedSolution,
    rank: usize,
    alt: &ProposedSolution,
) -> Markup {
    let diff = build_diff(primary, alt);
    let changed_count = diff.values().filter(|d| !matches!(d, SeatDiff::Same)).count();
    let used: Vec<&ProposedLineup> = alt.lineups.iter().filter(|l| l.used).collect();
    html! {
        div class="border border-slate-200 rounded-lg p-4" {
            div class="flex items-center space-x-3 mb-3" {
                h3 class="font-bold text-slate-700" { "Alternative #" (rank) }
                @if changed_count > 0 {
                    span class="text-xs bg-amber-100 text-amber-800 px-2 py-0.5 rounded-full" {
                        (changed_count) " seat"
                        @if changed_count != 1 { "s" }
                        " changed"
                    }
                }
            }
            @if used.is_empty() {
                div class="text-slate-500 italic" { "No boats fielded." }
            } @else {
                div class="grid grid-cols-1 md:grid-cols-2 gap-4" {
                    @for lineup in &used {
                        (boat_card(snapshot, lineup, Some(&diff)))
                    }
                }
            }
            (unplaced_block(snapshot, &alt.unplaced))
        }
    }
}

// =====================================================================
// Diff engine: compare alternative seat assignments against the primary
// =====================================================================

/// Per-seat diff against the primary lineup.
enum SeatDiff {
    /// Same rower in this seat as the primary.
    Same,
    /// Different rower; `was` is who held this seat in the primary.
    Changed { was: RowerId },
    /// Seat wasn't in the primary (boat not fielded or seat didn't exist).
    New,
}

type DiffMap = HashMap<(BoatId, i32), SeatDiff>;

/// Index every `(boat_id, seat) → rower` in the primary, then compare
/// each alt seat against it. O(seats) in both solutions.
fn build_diff(primary: &ProposedSolution, alt: &ProposedSolution) -> DiffMap {
    let mut primary_seats: HashMap<(BoatId, i32), RowerId> = HashMap::new();
    for lineup in &primary.lineups {
        if lineup.used {
            for &(seat, rower_id) in &lineup.seats {
                primary_seats.insert((lineup.boat_id, seat), rower_id);
            }
        }
    }

    let mut diff = DiffMap::new();
    for lineup in &alt.lineups {
        if lineup.used {
            for &(seat, rower_id) in &lineup.seats {
                let key = (lineup.boat_id, seat);
                let entry = match primary_seats.get(&key) {
                    Some(&primary_rower) if primary_rower == rower_id => SeatDiff::Same,
                    Some(&primary_rower) => SeatDiff::Changed { was: primary_rower },
                    None => SeatDiff::New,
                };
                diff.insert(key, entry);
            }
        }
    }
    diff
}

fn boat_card(
    snapshot: &DbSnapshot,
    lineup: &ProposedLineup,
    diff: Option<&DiffMap>,
) -> Markup {
    let seat_count = snapshot
        .sweep_boats
        .iter()
        .find(|b| b.id == lineup.boat_id)
        .map(|b| b.seat_count)
        .unwrap_or(0);
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
                        @let seat_diff = diff.and_then(|d| d.get(&(lineup.boat_id, *seat)));
                        (seat_row(snapshot, *seat, *rower_id, seat_diff))
                    }
                }
            }
        }
    }
}

fn seat_row(
    snapshot: &DbSnapshot,
    seat: i32,
    rower_id: RowerId,
    diff: Option<&SeatDiff>,
) -> Markup {
    let label = if seat == 0 {
        "cox".to_string()
    } else {
        format!("s{seat}")
    };
    let is_changed = matches!(diff, Some(SeatDiff::Changed { .. }) | Some(SeatDiff::New));
    let row_class = if is_changed {
        "border-b border-slate-100 last:border-0 bg-amber-50"
    } else {
        "border-b border-slate-100 last:border-0"
    };
    let rower = find_rower(snapshot, rower_id);
    html! {
        tr class=(row_class) {
            td class="px-4 py-2 text-slate-500 font-mono text-xs w-12" { (label) }
            td class="px-4 py-2" {
                @if let Some(r) = rower {
                    div class="font-medium text-slate-800" {
                        (r.name)
                        @if is_changed {
                            span class="ml-1 text-xs text-amber-700" { "●" }
                        }
                    }
                    div class="text-xs text-slate-500" {
                        (r.weight_class) " · " (r.skill) " · " (r.strength) " · " (r.side)
                    }
                    @if let Some(SeatDiff::Changed { was }) = diff {
                        @if let Some(prev) = find_rower(snapshot, *was) {
                            div class="text-xs text-amber-700 italic" {
                                "was " (prev.name)
                            }
                        }
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
