//! History list + detail templates.
//!
//! The detail view (`/history/{id}`) is the "committed practice" page —
//! a read-only lineup display with a bench sidebar, summary strip, and
//! boat cards styled with side-tinted seat tags.

use std::collections::HashSet;

use chrono::NaiveDate;
use std::collections::HashMap;

use lineup_db::{
    availability::types::AvailabilityStatus,
    boat::types::BoatId,
    lineup::CommittedLineup,
    oar_set::types::OarSetId,
    practice::{Practice, PracticeId},
    rower::{
        types::{RowerId, Side},
        Rower,
    },
    snapshot::DbSnapshot,
    timeline::BlockType,
};
use maud::{html, Markup};

use super::layout::crossed_oars_icon;

use super::layout::{empty_state, page_header};
use super::solve::{commit_meter, seat_badge, seat_label};

// ── List view ────────────────────────────────────────────────────────

pub(crate) fn list_content(practices: &[Practice], stale_ids: &HashSet<PracticeId>) -> Markup {
    html! {
        (page_header("Committed practices", Some("Lineups that have been saved and sent out.")))
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto" {
            @if practices.is_empty() {
                (empty_state("No practices committed yet."))
            } @else {
                div class="bg-paper rounded-lg shadow-soft divide-y divide-rule-2" {
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
          class="flex items-center justify-between px-6 py-4 hover:bg-paper-2 transition cursor-pointer"
          hx-get=(href)
          hx-target="#content"
          hx-push-url="true" {
            div {
                div class="flex items-center gap-2" {
                    span class="font-semibold text-ink" { (p.date) }
                    @if is_stale {
                        span class="text-xs bg-warn/15 text-warn px-1.5 py-0.5 rounded-full" {
                            "Availability changed"
                        }
                    }
                }
                div class="text-sm text-ink-3" {
                    (weekday)
                    @if let Some(ref notes) = p.notes {
                        @if !notes.is_empty() {
                            " — "
                            span class="text-muted italic" {
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
            span class="text-muted" "aria-hidden"="true" { "→" }
        }
    }
}

// ── Detail view ──────────────────────────────────────────────────────

pub(crate) fn detail_content(
    snapshot: &DbSnapshot,
    practice_id: PracticeId,
    date: NaiveDate,
    practice: Option<&Practice>,
    committed: &[CommittedLineup],
    force_cox_stern: bool,
    is_coach: bool,
    oar_assignments: &HashMap<BoatId, (OarSetId, String)>,
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

    // Compute summary stats.
    let boat_count = committed.len();
    let total_seats: usize = committed.iter().map(|c| c.seats.len()).sum();
    let placed_ids: HashSet<RowerId> = committed
        .iter()
        .flat_map(|c| c.seats.iter().map(|s| s.rower_id))
        .collect();

    // Bench: available but not placed.
    let bench: Vec<&Rower> = snapshot
        .available_rowers()
        .filter(|r| !placed_ids.contains(&r.id))
        .collect();

    // No-response: rowers with no availability entry who are active.
    // Only relevant when assume_available is false (otherwise they'd be
    // in available_rowers). Also exclude placed rowers.
    let no_response: Vec<&Rower> = if snapshot.assume_available {
        vec![]
    } else {
        snapshot
            .rowers
            .iter()
            .filter(|r| r.active.as_bool())
            .filter(|r| !snapshot.availability.contains_key(&r.id))
            .filter(|r| !placed_ids.contains(&r.id))
            .collect()
    };

    // Unavailable: explicitly said No.
    let unavailable: Vec<&Rower> = snapshot
        .rowers
        .iter()
        .filter(|r| r.active.as_bool())
        .filter(|r| {
            snapshot
                .availability
                .get(&r.id)
                .map(|s| *s == AvailabilityStatus::No)
                .unwrap_or(false)
        })
        .filter(|r| !placed_ids.contains(&r.id))
        .collect();

    let weekday = date.format("%A").to_string();
    let date_display = date.format("%b %-d").to_string();

    html! {
        // ── Header ──
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            div class="flex items-center gap-3 mb-1" {
                a href="/practices"
                  class="font-mono-stat text-xs tracking-wider hover:underline"
                  style="color: var(--muted)"
                  hx-get="/practices"
                  hx-target="#content"
                  hx-push-url="true" {
                    "← All practices"
                }
                @if !is_cancelled {
                    span class="cv-status-pill" {
                        span class="cv-status-dot" {}
                        "committed"
                    }
                }
            }
            div class="flex items-center justify-between flex-wrap gap-3" {
                h1 class="font-serif-heading text-2xl font-medium tracking-tight flex items-baseline gap-2" style="color: var(--ink)" {
                    span class="font-normal" style="color: var(--ink-2)" { (weekday) "," }
                    span { (date_display) }
                    @if let Some(p) = practice {
                        @if let Some(t) = p.time {
                            span class="font-mono-stat text-sm font-normal ml-1" style="color: var(--muted)" {
                                "· " (t.format("%l:%M %p").to_string().trim())
                            }
                        }
                    }
                }
                @if is_coach && !committed.is_empty() && !is_cancelled {
                    div class="no-print flex items-center gap-2" {
                        form method="post" action=(cancel_action)
                             hx-post=(cancel_action)
                             hx-target="#content" {
                            button type="submit"
                                   class="btn-warm-ghost text-xs py-2" style="color: var(--muted)" {
                                "Cancel practice"
                            }
                        }
                        button type="button"
                               class="btn-warm-ghost text-sm py-2"
                               onclick=(edit_lineup_js(practice_id, committed, snapshot)) {
                            "Edit lineup"
                        }
                        // Send lineups button — opens existing send-lineups preview modal
                        button type="button"
                               class="btn-warm-ink text-sm py-2 px-4"
                               hx-get={"/practices/lineup-preview?practice_id=" (practice_id) "&scope=practice"}
                               hx-target="body"
                               hx-swap="beforeend" {
                            "Send lineups"
                        }
                    }
                }
            }
        }

        // ── Summary strip ──
        div class="flex items-stretch gap-0 px-4 sm:px-8 py-3 border-b flex-wrap" style="border-color: var(--rule); background: var(--paper)" {
            div class="flex items-baseline gap-3 pr-6" {
                span class="cv-stat-num font-serif-heading" { (boat_count) }
                div class="flex flex-col gap-0.5" {
                    span class="font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "boats" }
                }
            }
            div class="cv-stat-sep" {}
            div class="flex items-baseline gap-3 pr-6" {
                span class="cv-stat-num font-serif-heading" { (total_seats) }
                div class="flex flex-col gap-0.5" {
                    span class="font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "rowing" }
                }
            }
            div class="cv-stat-sep" {}
            div class="flex items-baseline gap-3 pr-6" {
                span class="cv-stat-num font-serif-heading" { (bench.len()) }
                div class="flex flex-col gap-0.5" {
                    span class="font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "on bench" }
                    @if !no_response.is_empty() {
                        span class="font-mono-stat text-[9px] tracking-wide" style="color: var(--muted)" {
                            (no_response.len()) " no response"
                        }
                    }
                }
            }
        }

        // ── Body: bench sidebar + main content ──
        div class="cv-layout" {
            // Bench sidebar
            (bench_sidebar(&bench, &no_response, &unavailable))

            // Main content
            div class="px-4 sm:px-8 py-5 min-w-0" {
                @if is_cancelled {
                    div class="mb-4 rounded px-4 py-3 text-sm flex items-center justify-between" style="background: color-mix(in oklch, var(--bad) 10%, var(--paper)); border-left: 4px solid var(--bad); color: var(--ink)" {
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
                                       class="text-sm font-semibold underline" style="color: var(--bad)" {
                                    "Restore"
                                }
                            }
                        }
                    }
                }

                // Practice notes
                @if is_coach {
                    div class="no-print mb-5" {
                        (notes_section(practice, practice_id))
                    }
                }

                // Plan gate: prompt coach to build a plan or dismiss
                @if is_coach && !committed.is_empty() {
                    @let has_plan = practice.and_then(|p| p.timeline()).is_some();
                    @let plan_dismissed = practice.map(|p| p.plan_dismissed.as_bool()).unwrap_or(false);
                    @if !has_plan && !plan_dismissed {
                        div class="mb-5 no-print rounded px-4 py-3 text-sm flex items-center justify-between"
                             style="background: color-mix(in oklch, var(--accent) 8%, var(--paper)); border-left: 4px solid var(--accent); color: var(--ink)" {
                            div {
                                strong { "Practice plan. " }
                                "Build a practice plan below, or skip if you don't need one."
                            }
                            form method="post" action={"/practices/" (practice_id) "/dismiss-plan"}
                                 hx-post={"/practices/" (practice_id) "/dismiss-plan"}
                                 hx-target="#content" {
                                button type="submit"
                                       class="text-sm font-semibold underline ml-4 shrink-0" style="color: var(--accent)" {
                                    "Skip plan"
                                }
                            }
                        }
                    }
                    @if plan_dismissed && !has_plan {
                        div class="mb-5 no-print rounded px-4 py-3 text-sm flex items-center justify-between"
                             style="background: var(--paper-2); border-left: 4px solid var(--rule); color: var(--muted)" {
                            span { "Plan skipped. You can still build one below." }
                        }
                    }
                }

                // Practice timeline summary
                @if is_coach {
                    @let timeline = practice.and_then(|p| p.timeline());
                    div class="mb-5 no-print" {
                        @if let Some(ref tl) = timeline {
                            (super::timeline::summary(tl, practice_id))
                        } @else {
                            @let default_tl = lineup_db::timeline::Timeline::default_empty(
                                practice.and_then(|p| p.duration_minutes.map(|d| d.as_int() as u32)).unwrap_or(90)
                            );
                            (super::timeline::summary(&default_tl, practice_id))
                        }
                    }
                }

                @if has_stale {
                    div class="mb-4 rounded px-4 py-3 text-sm" style="background: color-mix(in oklch, var(--warn) 10%, var(--paper)); border-left: 4px solid var(--warn); color: var(--ink)" {
                        strong { "Availability changed. " }
                        "One or more rowers in this lineup are no longer available. "
                        "Highlighted rowers may need to be substituted."
                    }
                }

                @if committed.is_empty() {
                    (empty_state("No lineups committed for this date."))
                } @else if is_coach {
                    div class="rounded-lg pt-1 pb-2" style="border-top: 3px solid var(--accent)" {
                        span class="font-mono-stat text-[9.5px] tracking-[0.16em] uppercase font-semibold block py-2 pl-4" style="color: var(--accent)" {
                            "Lineups"
                        }
                        form id="noshow-form" method="get" action={"/solve/" (practice_id)} {
                            div class="grid gap-4" style="grid-template-columns: repeat(auto-fit, minmax(400px, 1fr))" {
                                @for c in committed {
                                    (boat_card(snapshot, c, force_cox_stern, &stale_rowers, is_coach, oar_assignments.get(&c.lineup.boat_id).map(|(_, name)| name.as_str())))
                                }
                            }
                        }
                    }
                } @else {
                    div class="rounded-lg pt-1 pb-2" style="border-top: 3px solid var(--accent)" {
                        span class="font-mono-stat text-[9.5px] tracking-[0.16em] uppercase font-semibold block py-2 pl-4" style="color: var(--accent)" {
                            "Lineups"
                        }
                        div class="grid gap-4" style="grid-template-columns: repeat(auto-fit, minmax(400px, 1fr))" {
                            @for c in committed {
                                (boat_card(snapshot, c, force_cox_stern, &stale_rowers, is_coach, oar_assignments.get(&c.lineup.boat_id).map(|(_, name)| name.as_str())))
                            }
                        }
                    }
                }

                // Footer
                @if !committed.is_empty() {
                    div class="mt-6 pt-3 flex justify-between items-baseline font-mono-stat text-xs" style="border-top: 1px solid var(--rule-2); color: var(--muted)" {
                        span {
                            "Lineup committed "
                            strong style="color: var(--ink-2)" { (committed[0].lineup.created_at) }
                        }
                    }
                }
            }

            // Right spacer — mirrors bench sidebar width for visual balance
            div class="cv-spacer" {}
        }
    }
}

// ── Notes section ────────────────────────────────────────────────────

fn notes_section(practice: Option<&Practice>, practice_id: PracticeId) -> Markup {
    let existing_notes = practice.and_then(|p| p.notes.as_deref()).unwrap_or("");
    html! {
        div id="practice-notes" "aria-live"="polite" {
            (notes_display_inner(existing_notes, practice_id))
        }
    }
}

/// Rendered by the HTMX swap after saving notes.
pub(crate) fn notes_display(practice: &Practice) -> Markup {
    let notes = practice.notes.as_deref().unwrap_or("");
    html! {
        div id="practice-notes" "aria-live"="polite" {
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
            class="rounded-lg p-4"
            style="border-left: 3px solid var(--accent); max-width: 720px"
        {
            div class="flex items-baseline justify-between mb-1 gap-2" {
                span class="font-mono-stat text-[9.5px] tracking-[0.16em] uppercase font-semibold" style="color: var(--accent)" {
                    "Practice notes"
                }
            }
            textarea
                name="notes"
                rows="2"
                placeholder="Add notes for this practice…"
                class="w-full border rounded px-3 py-2 text-sm focus:outline-none resize-y"
                style="background: var(--paper-2); border-color: var(--rule); color: var(--ink)"
            {
                (notes)
            }
            div class="mt-2 flex justify-end" {
                button
                    type="submit"
                    class="btn-warm-ink text-xs py-1.5 px-3"
                {
                    "Save notes"
                }
            }
        }
    }
}

// ── Boat card ────────────────────────────────────────────────────────

fn boat_card(
    snapshot: &DbSnapshot,
    committed: &CommittedLineup,
    force_cox_stern: bool,
    stale_rowers: &HashSet<RowerId>,
    is_coach: bool,
    oar_set_name: Option<&str>,
) -> Markup {
    let boat = snapshot
        .boats
        .iter()
        .find(|b| b.id == committed.lineup.boat_id);
    let boat_name = boat.map(|b| b.name.as_str()).unwrap_or("<unknown boat>");
    let cox_at_top = force_cox_stern || boat.map(|b| b.cox_position.cox_first()).unwrap_or(true);
    let seat_count = boat.map(|b| b.seat_count.as_int()).unwrap_or(0);
    let has_cox = boat.map(|b| b.has_cox.as_bool()).unwrap_or(false);
    let is_sweep = boat.map(|b| b.is_sweep()).unwrap_or(true);

    // Build full seat list.
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

    let filled = committed.seats.len();

    // Rig label
    let rig = if is_sweep {
        boat.map(super::solve::rig_label).unwrap_or("")
    } else {
        "sculling"
    };

    // Weight class label
    let weight_class = boat
        .map(|b| format!("{}", b.weight_class))
        .unwrap_or_default();

    // Cox position label — only show for non-default (bow-loader) positions.
    // 8+s are always stern-loaded (default), 4+s can be bow or stern.
    let cox_pos_label = if has_cox {
        boat.and_then(|b| {
            if b.cox_position == lineup_db::boat::types::CoxPosition::Bow {
                Some("bow-loader".to_string())
            } else {
                None // stern is default, elide
            }
        })
        .unwrap_or_default()
    } else {
        String::new()
    };

    // Boat type label (e.g. "8+", "4x")
    let type_label = boat
        .map(|b| {
            let sc = b.seat_count.as_int();
            let oars = b.oars_per_seat.as_int();
            let cox_sym = if b.has_cox.as_bool() {
                "+"
            } else if oars == 2 {
                "x"
            } else {
                "-"
            };
            if oars == 2 {
                format!("{sc}x")
            } else if b.has_cox.as_bool() {
                format!("{sc}+")
            } else {
                format!("{sc}{cox_sym}")
            }
        })
        .unwrap_or_default();

    html! {
        article class="cv-boat print-break" {
            // Boat header
            header class="cv-boat-head" {
                h2 class="font-serif-heading text-xl font-medium tracking-tight m-0" style="color: var(--ink)" {
                    (boat_name)
                }
                div class="mt-1 font-mono-stat text-[10.5px] flex flex-wrap gap-1.5 items-center" style="color: var(--muted)" {
                    span class="stat-badge stat-tier-2 text-[9px]" { (type_label) }
                    @if !weight_class.is_empty() {
                        span { (weight_class.to_lowercase()) }
                    }
                    span style="color: var(--rule)" { "·" }
                    @if is_sweep {
                        @if let Some(b) = boat {
                            span class="inline-flex items-center gap-0.5" {
                                (super::layout::rigger_icon(b.stroke_side))
                                (rig)
                            }
                        } @else {
                            span { (rig) }
                        }
                    } @else {
                        span { (rig) }
                    }
                    @if !cox_pos_label.is_empty() {
                        span style="color: var(--rule)" { "·" }
                        span { (cox_pos_label.to_lowercase()) }
                    }
                    @if let Some(oars) = oar_set_name {
                        span style="color: var(--rule)" { "·" }
                        span class="inline-flex items-center gap-1" {
                            (crossed_oars_icon())
                            (oars) " oars"
                        }
                    }
                    span style="color: var(--rule)" { "·" }
                    span class="inline-flex items-center gap-1" {
                        (super::layout::seat_icon())
                        (filled) "/" (all_positions.len()) " seated"
                    }
                }
            }

            // Seat rows
            div class="px-3 sm:px-4 py-2" {
                // Stern endcap
                div class="end-cap my-1" {
                    span class="end-cap-rule" {}
                    span class="px-1" { "stern" }
                    span class="end-cap-rule" {}
                }

                @for pos in &all_positions {
                    @let label = seat_label(*pos, seat_count);
                    @let maybe_seat = seat_map.get(pos);
                    @let rower = maybe_seat.and_then(|s| snapshot.rowers.iter().find(|r| r.id == s.rower_id));
                    @let is_cox_seat = *pos == 0;
                    @let is_stale = maybe_seat.map(|s| stale_rowers.contains(&s.rower_id)).unwrap_or(false);
                    @let is_mismatch = !is_cox_seat && rower.map(|r| {
                        if let Some(b) = boat {
                            if let Some(seat_side) = b.seat_side(*pos) {
                                r.side != Side::Either && r.side != seat_side
                            } else { false }
                        } else { false }
                    }).unwrap_or(false);

                    div class={
                        "cv-seat"
                        @if is_stale { " cv-seat-stale" }
                    } {
                        // Seat tag (colored by side)
                        (seat_badge(boat, *pos, &label))

                        // Rower name + metadata
                        div class="min-w-0" {
                            @if let Some(r) = rower {
                                div class="float-right ml-2" { (commit_meter(r)) }
                                span class="font-serif-heading font-medium text-[15px] tracking-tight" style="color: var(--ink)" {
                                    (r.display_name())
                                }
                                @if is_stale {
                                    span class="ml-1 font-mono-stat text-[9px] tracking-wide px-1.5 py-0.5 rounded-full" style="background: color-mix(in oklch, var(--warn) 20%, var(--paper)); color: var(--warn)" {
                                        "unavailable"
                                    }
                                }
                                @if is_mismatch {
                                    span class="ml-1 cv-offside" title="Rower's preferred side doesn't match this seat" {
                                        "off-side"
                                    }
                                }
                            } @else {
                                span class="font-mono-stat text-xs italic" style="color: var(--muted)" {
                                    "empty"
                                }
                            }
                        }

                        // No-show toggle (coach only)
                        @if is_coach {
                            @if let Some(seat) = maybe_seat {
                                label class="no-print cursor-pointer"
                                      onclick="var s=this.querySelector('span');var c=this.querySelector('input');setTimeout(function(){s.style.color=c.checked?'var(--bad)':'var(--muted)';s.style.borderColor=c.checked?'var(--bad)':'var(--rule)';s.style.background=c.checked?'color-mix(in oklch, var(--bad) 10%, var(--paper))':'var(--paper)'},0)" {
                                    input type="checkbox" name="no_show" value=(seat.rower_id)
                                          class="hidden";
                                    span class="inline-flex items-center font-mono-stat text-[9px] tracking-wide uppercase px-2 py-1 rounded border"
                                         style="color: var(--muted); border-color: var(--rule); background: var(--paper); transition: all 0.1s" {
                                        "No-show"
                                    }
                                }
                            } @else {
                                span {}
                            }
                        }
                    }
                }

                // Bow endcap
                div class="end-cap my-1" {
                    span class="end-cap-rule" {}
                    span class="px-1" { "bow" }
                    span class="end-cap-rule" {}
                }
            }
        }
    }
}

// ── Bench sidebar ────────────────────────────────────────────────────

fn bench_sidebar(bench: &[&Rower], no_response: &[&Rower], unavailable: &[&Rower]) -> Markup {
    html! {
        aside class="cv-bench" {
            div class="px-4 pt-3 pb-2 flex items-baseline justify-between" style="border-bottom: 1px solid var(--rule-2)" {
                h3 class="font-serif-heading font-medium text-base tracking-tight m-0" {
                    "Bench"
                }
                span class="font-mono-stat text-[9.5px] tracking-widest uppercase" style="color: var(--muted)" {
                    (bench.len()) " avail"
                    @if !no_response.is_empty() {
                        " · " (no_response.len()) " no resp"
                    }
                }
            }
            div class="cv-bench-list" {
                // Available section
                @if !bench.is_empty() {
                    (bench_section("Available", bench, false))
                }

                // No response section
                @if !no_response.is_empty() {
                    (bench_section("No response", no_response, true))
                }

                // Unavailable section
                @if !unavailable.is_empty() {
                    (bench_section("Unavailable", unavailable, true))
                }

                @if bench.is_empty() && no_response.is_empty() && unavailable.is_empty() {
                    div class="py-5 text-center font-mono-stat text-xs italic" style="color: var(--muted)" {
                        "Everyone is rowing"
                    }
                }
            }
        }
    }
}

fn bench_section(title: &str, rowers: &[&Rower], dimmed: bool) -> Markup {
    html! {
        div class="mt-2" {
            div class="flex items-baseline justify-between px-2 py-1" {
                span class="font-mono-stat text-[9.5px] tracking-wider uppercase font-semibold" style="color: var(--muted)" {
                    (title)
                }
                span class="font-mono-stat text-[9.5px]" style="color: var(--muted)" {
                    (rowers.len())
                }
            }
            div class="flex flex-col gap-px" {
                @for r in rowers {
                    div class={
                        "grid items-center gap-2 px-2 py-1 rounded"
                        @if dimmed { " opacity-55" }
                    } style="grid-template-columns: 28px 1fr" {
                        span class="flex justify-center" {
                            (rower_side_badge(r))
                        }
                        div class="min-w-0" {
                            div class="float-right ml-2" { (commit_meter(r)) }
                            span class="font-serif-heading font-medium text-[13px]" style="color: var(--ink)" {
                                (r.display_name())
                            }
                        }
                    }
                }
            }
        }
    }
}

fn rower_side_badge(r: &Rower) -> Markup {
    let (class, label) = if r.is_designated_cox.as_bool() {
        ("cv-side-badge cv-side-cox", "cox")
    } else {
        match r.side {
            Side::Port => ("cv-side-badge cv-side-port", "P"),
            Side::Starboard => ("cv-side-badge cv-side-stbd", "S"),
            Side::Either => ("cv-side-badge cv-side-either", "E"),
        }
    };
    html! {
        span class=(class) { (label) }
    }
}

// ── Timeline summary ─────────────────────────────────────────────────

/// CSS class for a block type's color chip.
pub(crate) fn block_type_css(bt: BlockType) -> &'static str {
    match bt {
        BlockType::Launch | BlockType::Dock => "color: var(--cox); background: color-mix(in oklch, var(--cox) 12%, var(--paper)); border-color: color-mix(in oklch, var(--cox) 28%, var(--rule))",
        BlockType::Rest => "color: var(--muted); background: var(--paper-2); border-color: var(--rule)",
        BlockType::Turn => "color: var(--warn); background: color-mix(in oklch, var(--warn) 12%, var(--paper)); border-color: color-mix(in oklch, var(--warn) 28%, var(--rule))",
    }
}

// ── Edit lineup JS ───────────────────────────────────────────────────

/// Build JS for the "Edit lineup" button.
fn edit_lineup_js(
    practice_id: PracticeId,
    committed: &[CommittedLineup],
    snapshot: &DbSnapshot,
) -> String {
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
    let _ = snapshot;

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
