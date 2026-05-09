//! Alternatives panel — diff-highlighted alternative lineups compared
//! against the primary solution.

use std::collections::HashMap;

use lineup_db::{
    boat::{types::BoatId, Boat},
    practice::PracticeId,
    rower::types::RowerId,
    snapshot::DbSnapshot,
};
use lineup_solver::{ProposedLineup, ProposedSolution, UnplacedRowers};
use maud::{html, Markup};

use super::editor::DisplayFlags;
use super::{
    cox_first, find_rower, rig_label, rower_stats_line_with_erg, seat_badge, seat_label,
    sort_seats_for_display,
};

#[allow(dead_code)] // kept for potential non-streaming fallback
pub(super) fn alternatives_panel(
    snapshot: &DbSnapshot,
    practice_id: PracticeId,
    primary: &ProposedSolution,
    alternatives: &[ProposedSolution],
    flags: &DisplayFlags,
) -> Markup {
    html! {
        section class="bg-white rounded-lg shadow p-6"
                x-data="{ open: false }" {
            button type="button"
                   "@click"="open = !open"
                   ":aria-expanded"="open"
                   class="flex items-center space-x-2 text-slate-700 hover:text-slate-900 font-semibold" {
                span x-text="open ? '▼' : '▶'" "aria-hidden"="true" {}
                span {
                    "Show "
                    (alternatives.len())
                    " alternative"
                    @if alternatives.len() != 1 { "s" }
                }
            }

            div x-show="open" class="mt-4 space-y-6" {
                @for (idx, alt) in alternatives.iter().enumerate() {
                    (alternative_block(snapshot, practice_id, primary, idx + 2, alt, flags))
                }
            }
        }
    }
}

pub(crate) fn alternative_block(
    snapshot: &DbSnapshot,
    practice_id: PracticeId,
    primary: &ProposedSolution,
    rank: usize,
    alt: &ProposedSolution,
    flags: &DisplayFlags,
) -> Markup {
    let diff = build_diff(primary, alt);
    let changed_count = diff
        .values()
        .filter(|d| !matches!(d, SeatDiff::Same))
        .count();
    let used: Vec<&ProposedLineup> = alt.lineups.iter().filter(|l| l.used).collect();

    // Build a promote URL that loads this alternative into the editor.
    // Uses seat=rower:boat:seat + boat=id params — no pins/locks.
    let promote_url = {
        let mut params = Vec::new();
        for lineup in &used {
            params.push(format!("boat={}", lineup.boat_id));
            for (seat, rower_id) in &lineup.seats {
                params.push(format!("seat={}:{}:{}", rower_id, lineup.boat_id, seat));
            }
        }
        format!("/solve/{}?{}", practice_id, params.join("&"))
    };

    html! {
        div class="solve-card p-4" {
            div class="flex items-center justify-between mb-3" {
                div class="flex items-center space-x-3" {
                    h3 class="font-bold font-serif-heading text-ink" { "Alternative #" (rank) }
                    @if changed_count > 0 {
                        span class="text-xs px-2 py-0.5 rounded-full font-mono-stat"
                             style="background: color-mix(in oklch, var(--warn) 15%, var(--paper)); color: var(--warn); border: 1px solid color-mix(in oklch, var(--warn) 30%, var(--rule))" {
                            (changed_count) " seat"
                            @if changed_count != 1 { "s" }
                            " changed"
                        }
                    }
                }
                a href=(promote_url)
                  class="btn-warm-ghost text-sm font-semibold transition"
                  style="color: var(--accent); border-color: var(--accent)" {
                    "Use this"
                }
            }
            @if used.is_empty() {
                div class="italic text-muted" { "No boats fielded." }
            } @else {
                div class="grid grid-cols-1 xl:grid-cols-2 gap-4" {
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
    let boat = snapshot.boats.iter().find(|b| b.id == lineup.boat_id);
    let seat_count = boat.map(|b| b.seat_count.as_int()).unwrap_or(0);
    let mut seats = lineup.seats.clone();
    let cox_at_top = cox_first(snapshot, lineup.boat_id, flags.force_cox_stern);
    sort_seats_for_display(&mut seats, cox_at_top);

    html! {
        div class="solve-card overflow-hidden" {
            div class="px-4 py-2" style="border-bottom: 1px dashed var(--rule)" {
                strong class="font-serif-heading text-ink" { (lineup.boat_name) }
                span class="text-xs font-mono-stat ml-2 text-muted" {
                    "(" (seat_count) "+"
                    @if let Some(b) = boat {
                        ", " (b.weight_class) ", " (rig_label(b))
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
    let sc = boat.map(|b| b.seat_count.as_int()).unwrap_or(0);
    let label = seat_label(seat, sc);
    let is_changed = matches!(diff, Some(SeatDiff::Changed { .. }) | Some(SeatDiff::New));
    let row_class = if is_changed {
        "last:border-0 seat-changed"
    } else {
        "last:border-0"
    };
    let rower = find_rower(snapshot, rower_id);
    html! {
        tr class=(row_class) style={"border-bottom: 1px solid var(--rule-2)"} {
            td class="px-4 py-2 w-12" {
                (seat_badge(boat, seat, &label))
            }
            td class="px-4 py-2" {
                @if let Some(r) = rower {
                    div class="font-medium font-serif-heading text-sm text-ink" {
                        (r.display_name())
                        @if is_changed {
                            span class="ml-1 text-xs text-warn" "aria-hidden"="true" title="Changed from current lineup" { "\u{25CF}" }
                        }
                    }
                    (rower_stats_line_with_erg(r, flags.show_attributes, snapshot.erg_scores.as_ref()))
                    @if let Some(SeatDiff::Changed { was }) = diff {
                        @if let Some(prev) = find_rower(snapshot, *was) {
                            div class="text-xs font-mono-stat italic text-warn" {
                                "was " (prev.display_name())
                            }
                        }
                    }
                } @else {
                    span class="italic font-mono-stat text-muted" { "unknown rower #" (rower_id) }
                }
            }
        }
    }
}

fn unplaced_block(snapshot: &DbSnapshot, unplaced: &UnplacedRowers) -> Markup {
    if unplaced.benched.is_empty() {
        return html! {};
    }
    html! {
        div class="mt-4 pt-4 text-sm space-y-2" style="border-top: 1px solid var(--rule)" {
            div {
                strong class="font-mono-stat text-[10px] uppercase tracking-wide text-muted" { "Benched: " }
                span class="font-serif-heading text-ink-2" {
                    (name_list(snapshot, &unplaced.benched))
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
                (r.display_name())
            } @else {
                "#" (id)
            }
        }
    }
}
