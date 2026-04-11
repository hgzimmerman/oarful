//! History list + detail templates.

use std::collections::HashSet;

use chrono::NaiveDate;
use lineup_db::{
    lineup::CommittedLineup,
    practice::Practice,
    rower::{types::RowerId, Rower},
    snapshot::DbSnapshot,
};
use maud::{html, Markup};

use super::layout::{empty_state, page_header};
use super::solve::side_indicator;

pub(crate) fn list_content(practices: &[Practice]) -> Markup {
    html! {
        (page_header("Committed practices", Some("Lineups that have been committed to the database.")))
        div class="px-8 py-6 max-w-3xl" {
            @if practices.is_empty() {
                (empty_state("No practices committed yet."))
            } @else {
                div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                    @for p in practices {
                        (row(p))
                    }
                }
            }
        }
    }
}

fn row(p: &Practice) -> Markup {
    let href = format!("/history/{}", p.date);
    let weekday = p.date.format("%A").to_string();
    html! {
        a href=(href)
          class="flex items-center justify-between px-6 py-4 hover:bg-slate-50 transition cursor-pointer"
          hx-get=(href)
          hx-target="#content"
          hx-push-url="true" {
            div {
                div class="font-semibold text-slate-800" { (p.date) }
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
    date: NaiveDate,
    practice: Option<&Practice>,
    committed: &[CommittedLineup],
    force_cox_stern: bool,
) -> Markup {
    html! {
        (page_header(&format!("History · {date}"), None))
        div class="px-8 py-6 max-w-4xl space-y-4" {
            (notes_section(practice, date))

            @if committed.is_empty() {
                (empty_state("No lineups committed for this date."))
            } @else {
                // No-show form: wraps all lineups with checkboxes per
                // rower. Submitting navigates to the solve view with
                // the committed lineup as baseline + checked rowers
                // marked as no-show.
                form method="get" action={"/solve/" (date)} {
                    input type="hidden" name="based_on" value=(date);
                    input type="hidden" name="similarity" value="3";
                    @for c in committed {
                        (lineup_block_with_noshow(snapshot, c, force_cox_stern))
                    }
                    div class="mt-4 flex justify-end" {
                        button type="submit"
                               class="px-4 py-2 text-sm bg-amber-600 text-white rounded hover:bg-amber-700 transition font-semibold" {
                            "Re-solve without no-shows"
                        }
                    }
                }
                (unplaced_section(snapshot, committed))
            }
        }
    }
}

fn notes_section(practice: Option<&Practice>, date: NaiveDate) -> Markup {
    let existing_notes = practice.and_then(|p| p.notes.as_deref()).unwrap_or("");
    html! {
        div id="practice-notes" {
            (notes_display_inner(existing_notes, date))
        }
    }
}

/// Rendered by the HTMX swap after saving notes.
pub(crate) fn notes_display(practice: &Practice, date: NaiveDate) -> Markup {
    let notes = practice.notes.as_deref().unwrap_or("");
    html! {
        div id="practice-notes" {
            (notes_display_inner(notes, date))
        }
    }
}

fn notes_display_inner(notes: &str, date: NaiveDate) -> Markup {
    let action = format!("/history/{date}/notes");
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

fn lineup_block_with_noshow(snapshot: &DbSnapshot, committed: &CommittedLineup, force_cox_stern: bool) -> Markup {
    let boat = snapshot
        .sweep_boats
        .iter()
        .find(|b| b.id == committed.lineup.boat_id);
    let boat_name = boat.map(|b| b.name.as_str()).unwrap_or("<unknown boat>");
    let cox_at_top = force_cox_stern
        || boat.map(|b| b.cox_position.cox_first()).unwrap_or(true);
    let mut seats = committed.seats.clone();
    seats.sort_by_key(|s| {
        if s.seat_position == 0 {
            if cox_at_top { i32::MIN } else { i32::MAX }
        } else {
            -s.seat_position
        }
    });

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
                        @let label = if seat.seat_position == 0 {
                            "cox".to_string()
                        } else {
                            format!("s{}", seat.seat_position)
                        };
                        @let rower = snapshot.rowers.iter().find(|r| r.id == seat.rower_id);
                        @let name = rower.map(|r| r.name.as_str()).unwrap_or("<unknown>");
                        @let is_designated_cox = rower.map(|r| r.is_designated_cox.as_bool()).unwrap_or(false);
                        @let row_class = if is_designated_cox {
                            "border-b border-slate-100 last:border-0 border-l-4 border-l-indigo-400"
                        } else {
                            "border-b border-slate-100 last:border-0"
                        };
                        tr class=(row_class) {
                            td class="px-4 py-2 text-slate-500 font-mono text-xs w-12" { (label) }
                            td class="px-4 py-2 text-slate-800" { (name) }
                            td class="px-4 py-2 text-right w-16" {
                                label class="inline-flex items-center gap-1 text-xs text-slate-500 cursor-pointer" {
                                    input type="checkbox" name="no_show" value=(seat.rower_id)
                                          class="rounded border-slate-300 text-amber-600 focus:ring-amber-500";
                                    "No-show"
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

/// Show rowers who were available but not placed in any committed
/// lineup — re-derived from the snapshot by subtracting placed rowers.
fn unplaced_section(snapshot: &DbSnapshot, committed: &[CommittedLineup]) -> Markup {
    let placed: HashSet<RowerId> = committed
        .iter()
        .flat_map(|c| c.seats.iter().map(|s| s.rower_id))
        .collect();

    let mut to_sculling: Vec<&Rower> = Vec::new();
    let mut benched: Vec<&Rower> = Vec::new();

    for r in snapshot.available_rowers() {
        if placed.contains(&r.id) {
            continue;
        }
        if r.can_scull.as_bool() {
            to_sculling.push(r);
        } else {
            benched.push(r);
        }
    }

    if to_sculling.is_empty() && benched.is_empty() {
        return html! {};
    }

    html! {
        div class="bg-white rounded-lg shadow p-4 text-sm space-y-2" {
            @if !to_sculling.is_empty() {
                div {
                    strong class="text-slate-700" { "To sculling: " }
                    span class="text-slate-600" {
                        @for (i, r) in to_sculling.iter().enumerate() {
                            @if i > 0 { ", " }
                            (r.name)
                        }
                    }
                }
            }
            @if !benched.is_empty() {
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
}
