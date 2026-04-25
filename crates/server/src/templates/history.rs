//! History list + detail templates.

use std::collections::HashSet;

use chrono::NaiveDate;
use lineup_db::{
    lineup::CommittedLineup,
    practice::{Practice, PracticeId},
    rower::{types::RowerId, Rower},
    snapshot::DbSnapshot,
};
use maud::{html, Markup};

use super::layout::{empty_state, page_header};
use super::solve::{seat_badge, seat_label, side_indicator};

pub(crate) fn list_content(practices: &[Practice], stale_ids: &HashSet<PracticeId>) -> Markup {
    html! {
        (page_header("Committed practices", Some("Lineups that have been saved and sent out.")))
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto" {
            @if practices.is_empty() {
                (empty_state("No practices committed yet."))
            } @else {
                div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                    @for p in practices {
                        (row(p, stale_ids.contains(&p.id)))
                    }
                }
            }
        }
    }
}

fn row(p: &Practice, is_stale: bool) -> Markup {
    let href = format!("/history/{}", p.id);
    let weekday = p.date.format("%A").to_string();
    html! {
        a href=(href)
          class="flex items-center justify-between px-6 py-4 hover:bg-slate-50 transition cursor-pointer"
          hx-get=(href)
          hx-target="#content"
          hx-push-url="true" {
            div {
                div class="flex items-center gap-2" {
                    span class="font-semibold text-slate-800" { (p.date) }
                    @if is_stale {
                        span class="text-xs bg-amber-100 text-amber-800 px-1.5 py-0.5 rounded-full" {
                            "Availability changed"
                        }
                    }
                }
                div class="text-sm text-slate-500" {
                    (weekday)
                    @if let Some(ref notes) = p.notes {
                        @if !notes.is_empty() {
                            " — "
                            span class="text-slate-400 italic" {
                                @if notes.len() > 60 {
                                    (&notes[..60]) "…"
                                } @else {
                                    (notes)
                                }
                            }
                        }
                    }
                }
            }
            span class="text-slate-400" { "→" }
        }
    }
}

pub(crate) fn detail_content(
    snapshot: &DbSnapshot,
    practice_id: PracticeId,
    date: NaiveDate,
    practice: Option<&Practice>,
    committed: &[CommittedLineup],
    force_cox_stern: bool,
    is_coach: bool,
) -> Markup {
    // Detect stale rowers: committed but availability is no longer "Yes".
    let stale_rowers: HashSet<RowerId> = committed
        .iter()
        .flat_map(|c| c.seats.iter().map(|s| s.rower_id))
        .filter(|rid| {
            !snapshot
                .availability
                .get(rid)
                .map(|s| s.is_available())
                .unwrap_or(snapshot.assume_available)
        })
        .collect();
    let has_stale = !stale_rowers.is_empty();

    let is_cancelled = practice.map(|p| p.cancelled.as_bool()).unwrap_or(false);
    let cancel_action = format!("/practices/{practice_id}/cancel");

    html! {
        header class="bg-white border-b border-slate-200 px-4 sm:px-8 py-4 sm:py-6" {
            div class="flex items-center justify-between" {
                h1 class="text-2xl font-bold text-slate-800" {
                    "Lineups \u{00b7} " (date)
                }
                @if is_coach && !committed.is_empty() && !is_cancelled {
                    div class="no-print flex items-center gap-3" {
                        form method="post" action=(cancel_action)
                             hx-post=(cancel_action)
                             hx-target="#content" {
                            button type="submit"
                                   class="text-xs text-slate-400 hover:text-red-600 font-medium py-2" {
                                "Cancel practice"
                            }
                        }
                        button type="button"
                               class="px-4 py-2 text-sm bg-slate-700 text-white rounded hover:bg-slate-800 transition font-semibold"
                               onclick=(edit_lineup_js(practice_id, committed, snapshot)) {
                            "Edit lineup"
                        }
                    }
                }
            }
        }
        div class="px-4 sm:px-8 py-6 max-w-4xl mx-auto space-y-4" {
            @if is_cancelled {
                div class="bg-red-50 border-l-4 border-red-500 px-4 py-3 rounded text-sm text-red-900 flex items-center justify-between" {
                    div {
                        strong { "Cancelled. " }
                        "This practice has been cancelled."
                    }
                    @if is_coach {
                        form method="post" action=(cancel_action)
                             hx-post=(cancel_action)
                             hx-target="#content"
                             class="no-print" {
                            button type="submit"
                                   class="text-sm font-semibold text-red-700 hover:text-red-900 underline" {
                                "Restore"
                            }
                        }
                    }
                }
            }

            @if is_coach {
                div class="no-print" {
                    (notes_section(practice, practice_id))
                }
            }

            @if has_stale {
                div class="bg-amber-50 border-l-4 border-amber-500 px-4 py-3 rounded text-sm text-amber-900" {
                    strong { "Availability changed. " }
                    "One or more rowers in this lineup are no longer available. "
                    "Highlighted rowers may need to be substituted."
                }
            }

            @if committed.is_empty() {
                (empty_state("No lineups committed for this date."))
            } @else if is_coach {
                // No-show form (Coach+): wraps all lineups with
                // checkboxes per rower. Edit button opens the editor
                // with placements pre-loaded and no-shows emptied.
                form id="noshow-form" method="get" action={"/solve/" (practice_id)} {
                    div class="space-y-4" {
                        @for c in committed {
                            (lineup_block_with_noshow(snapshot, c, force_cox_stern, &stale_rowers, is_coach))
                        }
                    }
                (unplaced_section(snapshot, committed))
                }
            } @else {
                // Read-only view for members — no checkboxes or re-solve.
                div class="space-y-4" {
                    @for c in committed {
                        (lineup_block_with_noshow(snapshot, c, force_cox_stern, &stale_rowers, is_coach))
                    }
                }
                (unplaced_section(snapshot, committed))
            }
        }
    }
}

fn notes_section(practice: Option<&Practice>, practice_id: PracticeId) -> Markup {
    let existing_notes = practice.and_then(|p| p.notes.as_deref()).unwrap_or("");
    html! {
        div id="practice-notes" {
            (notes_display_inner(existing_notes, practice_id))
        }
    }
}

/// Rendered by the HTMX swap after saving notes.
pub(crate) fn notes_display(practice: &Practice) -> Markup {
    let notes = practice.notes.as_deref().unwrap_or("");
    html! {
        div id="practice-notes" {
            (notes_display_inner(notes, practice.id))
        }
    }
}

fn notes_display_inner(notes: &str, practice_id: PracticeId) -> Markup {
    let action = format!("/history/{practice_id}/notes");
    html! {
        form
            hx-post=(action)
            hx-target="#practice-notes"
            hx-swap="outerHTML"
            class="bg-white rounded-lg shadow p-4"
        {
            label class="block text-sm font-medium text-slate-700 mb-1" {
                "Practice notes"
            }
            textarea
                name="notes"
                rows="3"
                placeholder="Add notes for this practice…"
                class="w-full border border-slate-300 rounded px-3 py-2 text-sm text-slate-800 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
            {
                (notes)
            }
            div class="mt-2 flex justify-end" {
                button
                    type="submit"
                    class="px-3 py-1.5 text-sm bg-blue-600 text-white rounded hover:bg-blue-700 transition"
                {
                    "Save notes"
                }
            }
        }
    }
}

fn lineup_block_with_noshow(
    snapshot: &DbSnapshot,
    committed: &CommittedLineup,
    force_cox_stern: bool,
    stale_rowers: &HashSet<RowerId>,
    is_coach: bool,
) -> Markup {
    let boat = snapshot
        .boats
        .iter()
        .find(|b| b.id == committed.lineup.boat_id);
    let boat_name = boat.map(|b| b.name.as_str()).unwrap_or("<unknown boat>");
    let cox_at_top = force_cox_stern || boat.map(|b| b.cox_position.cox_first()).unwrap_or(true);
    let seat_count = boat.map(|b| b.seat_count.as_int()).unwrap_or(0);
    let has_cox = boat.map(|b| b.has_cox.as_bool()).unwrap_or(false);

    // Build full seat list (all positions), mapping to Option<rower>.
    let seat_map: std::collections::HashMap<i32, &lineup_db::lineup::LineupSeatRow> = committed
        .seats
        .iter()
        .map(|s| (s.seat_position.as_int(), s))
        .collect();
    let mut all_positions: Vec<i32> = Vec::new();
    if has_cox {
        all_positions.push(0);
    }
    for s in 1..=seat_count {
        all_positions.push(s);
    }
    all_positions.sort_by_key(|s| {
        if *s == 0 {
            if cox_at_top {
                i32::MIN
            } else {
                i32::MAX
            }
        } else {
            -*s
        }
    });

    html! {
        div class="bg-white rounded-lg shadow overflow-hidden print-break" {
            div class="bg-slate-100 px-4 py-2 border-b border-slate-200" {
                strong { (boat_name) }
                span class="text-xs text-slate-500 ml-2" {
                    "committed " (committed.lineup.created_at)
                }
            }
            table class="w-full text-sm" {
                tbody {
                    @for pos in &all_positions {
                        @let label = seat_label(*pos, seat_count);
                        @let maybe_seat = seat_map.get(pos);
                        @let rower = maybe_seat.and_then(|s| snapshot.rowers.iter().find(|r| r.id == s.rower_id));
                        @let is_designated_cox = rower.map(|r| r.is_designated_cox.as_bool()).unwrap_or(false);
                        @let is_stale = maybe_seat.map(|s| stale_rowers.contains(&s.rower_id)).unwrap_or(false);
                        @let row_class = if is_stale {
                            "border-b border-slate-100 last:border-0 bg-amber-50 border-l-4 border-l-amber-400"
                        } else if is_designated_cox {
                            "border-b border-slate-100 last:border-0 border-l-4 border-l-indigo-400"
                        } else {
                            "border-b border-slate-100 last:border-0"
                        };
                        tr class=(row_class) {
                            td class="px-2 sm:px-4 py-1.5 sm:py-2 w-10 sm:w-12" {
                                (seat_badge(boat, *pos, &label))
                            }
                            td class="px-2 sm:px-4 py-1.5 sm:py-2 text-slate-800 truncate" {
                                @if let Some(r) = rower {
                                    (r.name)
                                    @if is_stale {
                                        span class="ml-1 sm:ml-2 text-xs bg-amber-200 text-amber-800 px-1 sm:px-1.5 py-0.5 rounded-full" {
                                            "unavailable"
                                        }
                                    }
                                } @else {
                                    span class="text-slate-400 italic" { "\u{2014} empty \u{2014}" }
                                }
                            }
                            @if is_coach {
                                @if let Some(seat) = maybe_seat {
                                    td class="px-2 sm:px-4 py-1.5 sm:py-2 text-right w-20 no-print whitespace-nowrap" {
                                        label class="inline-flex items-center gap-1 text-xs text-slate-500 cursor-pointer" {
                                            input type="checkbox" name="no_show" value=(seat.rower_id)
                                                  class="rounded border-slate-300 text-amber-600 focus:ring-amber-500";
                                            "No-show"
                                        }
                                    }
                                } @else {
                                    td class="w-20" {}
                                }
                            }
                            (side_indicator(rower))
                        }
                    }
                }
            }
        }
    }
}

/// Build JS for the "Edit lineup" button. Constructs a URL to the
/// editor endpoint with the current placements, minus any no-show
/// checked rowers.
fn edit_lineup_js(
    practice_id: PracticeId,
    committed: &[CommittedLineup],
    snapshot: &DbSnapshot,
) -> String {
    // Pre-compute the placement params from committed lineups.
    let mut seat_params = Vec::new();
    let mut boat_params = Vec::new();
    for c in committed {
        let boat_id = c.lineup.boat_id;
        boat_params.push(format!("boat={}", boat_id));
        for s in &c.seats {
            seat_params.push(format!(
                "seat={}:{}:{}",
                s.rower_id, boat_id, s.seat_position
            ));
        }
    }
    let base_params = [boat_params.join("&"), seat_params.join("&")].join("&");
    // Also carry walk-on rowers that are present in the snapshot's
    // availability but not in the roster's normal availability.
    // (Walk-ons are already baked into the snapshot by the time we
    // get here, so we don't need to thread them explicitly.)

    let _ = snapshot; // used only for future walk-on threading

    format!(
        r#"(function(){{
            var noshows = new Set();
            document.querySelectorAll('#noshow-form input[name="no_show"]:checked').forEach(function(el){{
                noshows.add(el.value);
            }});
            var parts = '{base_params}'.split('&').filter(function(p){{
                if (!p.startsWith('seat=')) return true;
                var rid = p.split(':')[2];
                return !noshows.has(rid);
            }});
            noshows.forEach(function(rid){{
                parts.push('no_show=' + rid);
            }});
            window.location.href = '/solve/{practice_id}?' + parts.join('&');
        }})()"#
    )
}

/// Show rowers who were available but not placed in any committed
/// lineup — re-derived from the snapshot by subtracting placed rowers.
fn unplaced_section(snapshot: &DbSnapshot, committed: &[CommittedLineup]) -> Markup {
    let placed: HashSet<RowerId> = committed
        .iter()
        .flat_map(|c| c.seats.iter().map(|s| s.rower_id))
        .collect();

    let mut benched: Vec<&Rower> = Vec::new();

    for r in snapshot.available_rowers() {
        if placed.contains(&r.id) {
            continue;
        }
        benched.push(r);
    }

    if benched.is_empty() {
        return html! {};
    }

    html! {
        div class="bg-white rounded-lg shadow p-4 text-sm space-y-2" {
            div {
                strong class="text-slate-700" { "Benched: " }
                span class="text-slate-600" {
                    @for (i, r) in benched.iter().enumerate() {
                        @if i > 0 { ", " }
                        (r.name)
                    }
                }
            }
        }
    }
}
