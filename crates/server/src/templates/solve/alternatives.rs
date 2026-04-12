//! Alternatives panel — diff-highlighted alternative lineups compared
//! against the primary solution.

use std::collections::HashMap;

use lineup_db::{
    boat::{types::BoatId, Boat},
    rower::types::RowerId,
    snapshot::DbSnapshot,
};
use lineup_solver::{ProposedLineup, ProposedSolution, UnplacedRowers};
use maud::{html, Markup};

use super::{
    cox_first, find_rower, rig_label, rower_stats_line, seat_badge, seat_label,
    sort_seats_for_display,
};
use super::editor::DisplayFlags;

pub(super) fn alternatives_panel(
    snapshot: &DbSnapshot,
    primary: &ProposedSolution,
    alternatives: &[ProposedSolution],
    flags: &DisplayFlags,
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
                    (alternative_block(snapshot, primary, idx + 2, alt, flags))
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
    flags: &DisplayFlags,
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
                        (boat_card(snapshot, lineup, Some(&diff), flags))
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

/// Index every `(boat_id, seat) -> rower` in the primary, then compare
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
    flags: &DisplayFlags,
) -> Markup {
    let boat = snapshot
        .sweep_boats
        .iter()
        .find(|b| b.id == lineup.boat_id);
    let seat_count = boat.map(|b| b.seat_count).unwrap_or(0);
    let mut seats = lineup.seats.clone();
    let cox_at_top = cox_first(snapshot, lineup.boat_id, flags.force_cox_stern);
    sort_seats_for_display(&mut seats, cox_at_top);

    html! {
        div class="border border-slate-200 rounded-lg overflow-hidden" {
            div class="bg-slate-100 px-4 py-2 border-b border-slate-200" {
                strong class="text-slate-800" { (lineup.boat_name) }
                span class="text-xs text-slate-500 ml-2" {
                    "(" (seat_count) "+"
                    @if let Some(b) = boat {
                        ", " (rig_label(b))
                    }
                    ")"
                }
            }
            table class="w-full text-sm" {
                tbody {
                    @for (seat, rower_id) in &seats {
                        @let seat_diff = diff.and_then(|d| d.get(&(lineup.boat_id, *seat)));
                        (seat_row(snapshot, boat, *seat, *rower_id, seat_diff, flags))
                    }
                }
            }
        }
    }
}

fn seat_row(
    snapshot: &DbSnapshot,
    boat: Option<&Boat>,
    seat: i32,
    rower_id: RowerId,
    diff: Option<&SeatDiff>,
    flags: &DisplayFlags,
) -> Markup {
    let sc = boat.map(|b| b.seat_count).unwrap_or(0);
    let label = seat_label(seat, sc);
    let is_changed = matches!(diff, Some(SeatDiff::Changed { .. }) | Some(SeatDiff::New));
    let row_class = if is_changed {
        "border-b border-slate-100 last:border-0 bg-amber-50"
    } else {
        "border-b border-slate-100 last:border-0"
    };
    let rower = find_rower(snapshot, rower_id);
    html! {
        tr class=(row_class) {
            td class="px-4 py-2 w-12" {
                (seat_badge(boat, seat, &label))
            }
            td class="px-4 py-2" {
                @if let Some(r) = rower {
                    div class="font-medium text-slate-800" {
                        (r.name)
                        @if is_changed {
                            span class="ml-1 text-xs text-amber-700" { "●" }
                        }
                    }
                    (rower_stats_line(r, flags.show_attributes))
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
