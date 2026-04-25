//! Solver knobs form, preset bar, and status banners.

use chrono::NaiveDate;
use lineup_db::practice::PracticeId;
use lineup_db::snapshot::DbSnapshot;
use lineup_solver::{Diagnostic, SolveResult, SolveStatus};
use maud::{html, Markup};

use crate::handlers::solve::SolveKnobs;

/// Coach-tunable knobs (partial fill / novelty / alternatives / time
/// budget). Submitting hx-gets the same `/solve/{id}` URL with the
/// new query string, so the result is bookmarkable and the back
/// button works.
pub(super) fn knobs_form(
    practice_id: PracticeId,
    knobs: &SolveKnobs,
    practices: &[lineup_db::practice::Practice],
    has_generated: bool,
    custom_profiles: &[(String, Option<String>)],
    snapshot: &DbSnapshot,
    solve_result: Option<&SolveResult>,
) -> Markup {
    let has_eight = snapshot.boats.iter().any(|b| b.seat_count.as_int() >= 8);
    let button_label = if has_generated {
        "Re-generate"
    } else {
        "Generate lineup"
    };
    let action = format!("/solve/{practice_id}");

    // Last run summary
    let last_run_label = if let Some(result) = solve_result {
        if result.status == SolveStatus::Satisfied {
            let ms = result.elapsed.as_millis();
            if ms < 1000 {
                format!("{ms} ms")
            } else {
                format!("{:.1}s", result.elapsed.as_secs_f64())
            }
        } else {
            "\u{2014}".to_string()
        }
    } else {
        "\u{2014}".to_string()
    };

    html! {
        script {
            (maud::PreEscaped(include_str!("../js/knobs.js")))
        }
        div "x-data"="{ railOpen: true }" class="flex flex-col flex-1" {
            // Collapsed state — thin vertical label
            button type="button"
                   class="solver-rail-closed"
                   x-show="!railOpen"
                   "@click"="railOpen = true"
                   title="Open solver" {
                span class="vert" { "SOLVER" }
            }

            // Open rail
            aside class="solver-rail" x-show="railOpen" x-cloak {
                // Header
                div class="sr-head" {
                    div {
                        h2 class="font-serif-heading font-medium text-base m-0" style="color: var(--ink)" { "Solver" }
                        div class="font-mono-stat text-[10px] mt-0.5" style="color: var(--muted)" {
                            "Last run: " (last_run_label)
                        }
                    }
                    button type="button"
                           class="text-xl leading-none cursor-pointer"
                           style="color: var(--muted)"
                           "@click"="railOpen = false" {
                        "\u{00d7}"
                    }
                }

                form method="get" action=(action)
                     hx-get=(action)
                     hx-target="#solve-results"
                     hx-push-url="true"
                     hx-indicator="#solve-spinner" {

                    // Preset section — vertical list via HTMX swap
                    section class="sr-section" {
                        div class="flex justify-between items-baseline mb-2" {
                            span class="text-[10px] font-semibold uppercase tracking-wide" style="color: var(--muted)" { "Preset" }
                            @let active_preset = if knobs.preset.is_empty() { "balanced" } else { &knobs.preset };
                            @let new_url = format!("/solver-profile/edit?basis={active_preset}");
                            button type="button"
                                   class="text-xs cursor-pointer"
                                   style="color: var(--accent)"
                                   hx-get=(&new_url)
                                   hx-target="body"
                                   hx-swap="beforeend" {
                                "+ New"
                            }
                        }
                        (preset_bar(practice_id, knobs, custom_profiles))
                    }

                    // Partial fill + Alternatives (side-by-side)
                    section class="sr-section" {
                        div class="grid grid-cols-2 gap-3" {
                            @if has_eight {
                                div {
                                    div class="text-[10px] font-semibold uppercase tracking-wide mb-1" style="color: var(--muted)" {
                                        "Partial fill"
                                    }
                                    div class="seg-warm" {
                                        @for (val, lbl) in &[(0, "Off"), (1, "1"), (2, "2")] {
                                            @let active = knobs.partial == *val;
                                            @let cls = if active { "seg-warm-btn seg-warm-btn-on" } else { "seg-warm-btn" };
                                            button type="button" class=(cls)
                                                   onclick={"segmentedSelect(this, 'partial', " (val) ")"} { (lbl) }
                                        }
                                    }
                                    input type="hidden" name="partial" value=(knobs.partial);
                                }
                            }
                            div {
                                div class="text-[10px] font-semibold uppercase tracking-wide mb-1" style="color: var(--muted)" {
                                    "Alternatives"
                                }
                                div class="seg-warm" {
                                    @for n in 0..=3i64 {
                                        @let active = knobs.alts as i64 == n;
                                        @let cls = if active { "seg-warm-btn seg-warm-btn-on" } else { "seg-warm-btn" };
                                        button type="button" class=(cls)
                                               onclick={"segmentedSelect(this, 'alts', " (n) ")"} { (n) }
                                    }
                                }
                                input type="hidden" name="alts" value=(knobs.alts);
                            }
                        }
                    }

                    // Similarity + Novelty (side-by-side sliders)
                    section class="sr-section" {
                        div class="grid grid-cols-2 gap-3" {
                            // Similarity
                            @if !practices.is_empty() {
                                div {
                                    div class="flex justify-between items-baseline" {
                                        span class="text-[10px] font-semibold uppercase tracking-wide" style="color: var(--muted)" { "Similarity" }
                                        span class="font-mono-stat text-xs" style="color: var(--ink-2)" { (knobs.similarity) }
                                    }
                                    input name="similarity" type="range" min="0" max="10"
                                          value=(knobs.similarity)
                                          class="range-warm"
                                          oninput="knobChanged()";
                                    p class="text-[10px] italic mt-0.5" style="color: var(--muted)" { "Stay close to last lineup" }
                                }
                            }
                            // Novelty
                            div {
                                div class="flex justify-between items-baseline" {
                                    span class="text-[10px] font-semibold uppercase tracking-wide" style="color: var(--muted)" { "Novelty" }
                                    span #novelty-val class="font-mono-stat text-xs" style="color: var(--ink-2)" {
                                        @if knobs.novelty == 0 { "Off" } @else { (knobs.novelty) }
                                    }
                                }
                                input name="novelty" type="range" min="0" max="5"
                                      value=(knobs.novelty)
                                      class="range-warm"
                                      oninput="document.getElementById('novelty-val').textContent = this.value === '0' ? 'Off' : this.value; knobChanged()";
                                p class="text-[10px] italic mt-0.5" style="color: var(--muted)" { "Avoid recent lineups" }
                            }
                        }
                    }

                    // Based-on checkboxes
                    @if !practices.is_empty() {
                        section class="sr-section" {
                            div class="text-[10px] font-semibold uppercase tracking-wide mb-2" style="color: var(--muted)" { "Based on" }
                            div class="flex flex-col gap-1" {
                                @for p in practices.iter().take(5) {
                                    @let date_str = p.date.format("%Y-%m-%d").to_string();
                                    @let weekday = p.date.format("%a").to_string();
                                    @let checked = knobs.based_on.contains(&date_str);
                                    label class="inline-flex items-center gap-1.5 text-xs cursor-pointer" style="color: var(--ink-2)" {
                                        input type="checkbox" name="based_on" value=(date_str)
                                              checked[checked]
                                              class="rounded"
                                              style="border-color: var(--rule)"
                                              onchange="knobChanged()";
                                        (date_str) " (" (weekday) ")"
                                    }
                                }
                            }
                        }
                    }

                    // Time budget
                    section class="sr-section" {
                        div class="flex justify-between items-baseline" {
                            span class="text-[10px] font-semibold uppercase tracking-wide" style="color: var(--muted)" { "Time budget" }
                            span #budget-val class="font-mono-stat text-xs" style="color: var(--ink-2)" {
                                (knobs.budget) "s"
                            }
                        }
                        input name="budget" type="range" min="1" max="10"
                              value=(knobs.budget)
                              class="range-warm"
                              oninput="document.getElementById('budget-val').textContent = this.value + 's'; knobChanged()";
                        p class="text-[10px] italic mt-0.5" style="color: var(--muted)" { "Per alternative" }
                    }

                    // Hidden state
                    input type="hidden" name="generate" value="1";
                    @for w in &knobs.walkon {
                        input type="hidden" name="walkon" value=(w);
                    }
                    @for ns in &knobs.no_show {
                        input type="hidden" name="no_show" value=(ns);
                    }
                    div #editor-knob-state style="display:none" {
                        @for l in &knobs.lock {
                            input type="hidden" name="lock" value=(l);
                        }
                        @for p in &knobs.pin {
                            input type="hidden" name="pin" value=(p);
                        }
                        @for wp in &knobs.was_pin {
                            input type="hidden" name="was_pin" value=(wp);
                        }
                        @for b in &knobs.boat {
                            input type="hidden" name="boat" value=(b);
                        }
                        @for bp in &knobs.boat_pin {
                            input type="hidden" name="boat_pin" value=(bp);
                        }
                        @for bwp in &knobs.boat_was_pin {
                            input type="hidden" name="boat_was_pin" value=(bwp);
                        }
                        @for bl in &knobs.boat_lock {
                            input type="hidden" name="boat_lock" value=(bl);
                        }
                    }

                    // Generate button — sticky at bottom
                    div class="sr-run" {
                        button type="submit"
                               class="btn-accent w-full shadow transition" {
                            (button_label)
                        }
                        span #solve-spinner class="htmx-indicator text-xs block mt-1 text-center" style="color: var(--muted)" {
                            "Generating\u{2026}"
                        }
                        p class="text-[10px] italic mt-2" style="color: var(--muted)" {
                            "Locked seats are preserved. Unavailable rowers are skipped."
                        }
                    }
                }
            }
        }
    }
}

/// Build a URL that switches the preset while carrying all other knobs.
fn preset_url_with(practice_id: PracticeId, knobs: &SolveKnobs, new_preset: &str) -> String {
    let mut parts = vec![
        format!("preset={new_preset}"),
        format!("partial={}", knobs.partial),
        format!("novelty={}", knobs.novelty),
        format!("alts={}", knobs.alts),
        format!("budget={}", knobs.budget),
    ];
    if knobs.similarity > 0 {
        parts.push(format!("similarity={}", knobs.similarity));
    }
    for b in &knobs.based_on {
        parts.push(format!("based_on={b}"));
    }
    for l in &knobs.lock {
        parts.push(format!("lock={l}"));
    }
    for p in &knobs.pin {
        parts.push(format!("pin={p}"));
    }
    for wp in &knobs.was_pin {
        parts.push(format!("was_pin={wp}"));
    }
    for w in &knobs.walkon {
        parts.push(format!("walkon={w}"));
    }
    for bp in &knobs.boat_pin {
        parts.push(format!("boat_pin={bp}"));
    }
    for bwp in &knobs.boat_was_pin {
        parts.push(format!("boat_was_pin={bwp}"));
    }
    for bl in &knobs.boat_lock {
        parts.push(format!("boat_lock={bl}"));
    }
    format!("/solve/{practice_id}/preset-bar?{}", parts.join("&"))
}

/// Render the preset list for HTMX `outerHTML` swaps.
/// Called by `GET /solve/{id}/preset-bar` and inline in the solver rail.
pub(crate) fn preset_bar(
    practice_id: PracticeId,
    knobs: &SolveKnobs,
    custom_profiles: &[(String, Option<String>)],
) -> Markup {
    let current = if knobs.preset.is_empty() {
        "balanced"
    } else {
        &knobs.preset
    };
    html! {
        div #preset-bar {
            div class="flex flex-col gap-0.5" {
                @for (value, label) in &[
                    ("balanced", "Balanced"),
                    ("even_speed", "Even speed"),
                    ("tiered", "Tiered"),
                    ("random", "Random"),
                ] {
                    @let is_active = current == *value;
                    @let row_cls = if is_active { "preset-row-on" } else { "" };
                    @let preset_url = preset_url_with(practice_id, knobs, value);
                    div class={"flex items-center justify-between px-2 py-1.5 rounded cursor-pointer " (row_cls)}
                        style="border: 1px solid transparent"
                        hx-get=(preset_url)
                        hx-target="#preset-bar"
                        hx-swap="outerHTML" {
                        div class="flex items-baseline gap-2 min-w-0 flex-1" {
                            span class="font-serif-heading font-medium text-sm" style="color: var(--ink)" { (label) }
                            span class="font-mono-stat text-[8px] px-1 rounded" style="color: var(--muted); border: 1px solid var(--rule)" { "built-in" }
                        }
                        div class="flex gap-0.5 opacity-60" {
                            @let edit_url = format!("/solver-profile/edit?name={value}");
                            button type="button"
                                   class="text-[10px] px-1 rounded cursor-pointer"
                                   style="border: 1px solid var(--rule); color: var(--ink-2)"
                                   hx-get=(&edit_url)
                                   hx-target="body"
                                   hx-swap="beforeend"
                                   onclick="event.stopPropagation()" {
                                "\u{21E2}"
                            }
                        }
                    }
                }
                @for (name, description) in custom_profiles {
                    @let is_active = current == name;
                    @let row_cls = if is_active { "preset-row-on" } else { "" };
                    @let preset_url = preset_url_with(practice_id, knobs, name);
                    div class={"flex items-center justify-between px-2 py-1.5 rounded cursor-pointer " (row_cls)}
                        style="border: 1px solid transparent"
                        title=[description.as_deref()]
                        hx-get=(preset_url)
                        hx-target="#preset-bar"
                        hx-swap="outerHTML" {
                        span class="font-serif-heading font-medium text-sm" style="color: var(--accent)" { (name) }
                        div class="flex gap-0.5 opacity-60" {
                            @let edit_url = format!("/solver-profile/edit?name={name}");
                            button type="button"
                                   class="text-[10px] px-1 rounded cursor-pointer"
                                   style="border: 1px solid var(--rule); color: var(--ink-2)"
                                   hx-get=(&edit_url)
                                   hx-target="body"
                                   hx-swap="beforeend"
                                   onclick="event.stopPropagation()" {
                                "\u{270E}"
                            }
                        }
                    }
                }
            }
            input type="hidden" name="preset" value=(current);
        }
    }
}

/// Error-only status banner. Success metadata is shown inline in the
/// knobs form summary. This function only renders for failures.
pub(crate) fn status_banner(date: NaiveDate, result: &SolveResult) -> Markup {
    match result.status {
        SolveStatus::Satisfied => html! {},
        SolveStatus::Unsatisfiable => html! {
            div class="px-4 py-3 rounded text-sm"
                style="background: color-mix(in oklch, var(--bad) 10%, var(--paper)); border-left: 4px solid var(--bad); color: var(--ink)" {
                strong { "Unsatisfiable." }
                " No seat assignment exists under the current constraints for " (date) "."
                @if result.diagnostics.is_empty() {
                    " Check the roster, availability, and hard locks."
                } @else {
                    ul class="mt-2 ml-4 list-disc space-y-1" {
                        @for d in &result.diagnostics {
                            li { (diagnostic_message(d)) }
                        }
                    }
                }
            }
        },
        SolveStatus::Timeout => html! {
            div class="px-4 py-3 rounded text-sm"
                style="background: color-mix(in oklch, var(--warn) 12%, var(--paper)); border-left: 4px solid var(--warn); color: var(--ink)" {
                strong { "No result." }
                " Ran out of time without finding a valid lineup. Try increasing the time budget or relaxing constraints."
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
        Diagnostic::InvalidLock {
            rower_name,
            boat_name,
            seat,
            reason,
        } => {
            format!("Seat lock skipped: {rower_name} in seat {seat} on {boat_name} — {reason}.")
        }
    }
}
