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
        "Generate"
    };
    let action = format!("/solve/{practice_id}");
    html! {
        // Segmented button helper: update hidden input + toggle active style.
        // Knob helpers: segmentedSelect() updates hidden inputs +
        // toggles button styling without form submission. knobChanged()
        // clears stale solver metrics from the summary. presetClicked()
        // updates the preset label in the summary.
        script {
            (maud::PreEscaped(include_str!("../js/knobs.js")))
        }
        section class="solve-card" {
            @let preset_label = if knobs.preset.is_empty() { "Balanced" } else { &knobs.preset };
            @let initially_open = !has_generated;
            div "x-data"={"{ open: " (initially_open) " }"} {
                button type="button"
                       "@click"="open = !open"
                       class="w-full flex items-center justify-between px-6 py-4 cursor-pointer select-none transition text-left"
                       style="color: var(--ink)" {
                    div class="flex items-center gap-3 flex-wrap" {
                        h3 class="text-sm font-semibold font-serif-heading" { "Solver settings" }
                        span #knob-preset-label class="text-xs font-mono-stat" style="color: var(--muted)" { (preset_label) }
                        @if let Some(result) = solve_result {
                            @if result.status == SolveStatus::Satisfied {
                                @let elapsed_ms = result.elapsed.as_millis();
                                @let elapsed_label = if elapsed_ms < 1000 {
                                    format!("{elapsed_ms}ms")
                                } else {
                                    format!("{:.1}s", result.elapsed.as_secs_f64())
                                };
                                span #knob-metrics class="text-xs font-mono-stat" style="color: var(--muted)" {
                                    "\u{00b7} " (elapsed_label)
                                    @if let Some(obj) = result.objective {
                                        " \u{00b7} score " (obj)
                                    }
                                }
                            }
                        }
                    }
                    span class="inline-block w-2 h-2 transform transition-transform duration-200"
                         style="border-right: 2px solid var(--muted); border-bottom: 2px solid var(--muted)"
                         ":class"="open ? 'rotate-45' : 'rotate-[-45deg]'" {}
                }
                div "x-show"="open"
                    "x-transition:enter"="transition-all ease-out duration-300"
                    "x-transition:enter-start"="opacity-0 max-h-0"
                    "x-transition:enter-end"="opacity-100 max-h-[2000px]"
                    "x-transition:leave"="transition-all ease-in duration-300"
                    "x-transition:leave-start"="opacity-100 max-h-[2000px]"
                    "x-transition:leave-end"="opacity-0 max-h-0"
                    class="overflow-hidden" {
                div class="px-6 pb-6 pt-4" style="border-top: 1px solid var(--rule-2)" {
            form method="get" action=(action)
                 hx-get=(action)
                 hx-target="#solve-results"
                 hx-push-url="true"
                 hx-indicator="#solve-spinner"
                 "@htmx:before-request"="if ($event.detail.elt === $el) open = false" {

                // Based-on checkbox list + similarity weight
                @if !practices.is_empty() {
                    div class="mb-4" {
                        div class="flex flex-wrap gap-4 items-end" {
                            fieldset class="flex-1" {
                                legend class="block text-xs font-semibold uppercase tracking-wide mb-2" style="color: var(--ink-2)" {
                                    "Based on"
                                }
                                div class="flex flex-wrap gap-3" {
                                    @for p in practices.iter().take(5) {
                                        @let date_str = p.date.format("%Y-%m-%d").to_string();
                                        @let weekday = p.date.format("%a").to_string();
                                        @let checked = knobs.based_on.contains(&date_str);
                                        label class="inline-flex items-center gap-1.5 text-sm cursor-pointer" style="color: var(--ink-2)" {
                                            input type="checkbox" name="based_on" value=(date_str)
                                                  checked[checked]
                                                  class="rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                                                  onchange="knobChanged()";
                                            (p.date) " (" (weekday) ")"
                                        }
                                    }
                                }
                            }
                            div class="w-28" {
                                (knob_input(
                                    "similarity",
                                    "Similarity",
                                    knobs.similarity as i64,
                                    Some(0),
                                    Some("0 = off"),
                                ))
                            }
                        }
                    }
                }

                // Solver preset selector
                div #preset-bar class="mb-4" {
                    div class="block text-xs font-semibold uppercase tracking-wide mb-2" style="color: var(--ink-2)" {
                        "Solver preset"
                    }
                    (preset_buttons(practice_id, knobs, custom_profiles))
                    input type="hidden" name="preset" value=(if knobs.preset.is_empty() { "balanced" } else { &knobs.preset });
                }

                // Solver knobs
                div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-4 items-end" {
                    // Partial fill — only relevant when the fleet has an 8+
                    @if has_eight {
                    div {
                        label class="block text-xs font-semibold uppercase tracking-wide mb-1" style="color: var(--ink-2)" {
                            "Partial fill"
                        }
                        div class="seg-warm" {
                            @for (val, lbl) in &[(0, "Off"), (1, "1 empty"), (2, "2 empty")] {
                                @let active = knobs.partial == *val;
                                @let cls = if active {
                                    "seg-warm-btn seg-warm-btn-on"
                                } else {
                                    "seg-warm-btn"
                                };
                                button type="button" class=(cls)
                                       onclick={"segmentedSelect(this, 'partial', " (val) ")"} {
                                    (lbl)
                                }
                            }
                        }
                        input type="hidden" name="partial" value=(knobs.partial);
                        p class="text-xs mt-1 italic" style="color: var(--muted)" { "Empty optional seats per boat" }
                    }
                    }

                    // Alternatives — segmented (0 / 1 / 2 / 3)
                    div {
                        label class="block text-xs font-semibold uppercase tracking-wide mb-1" style="color: var(--ink-2)" {
                            "Alternatives"
                        }
                        div class="seg-warm" {
                            @for n in 0..=3i64 {
                                @let active = knobs.alts as i64 == n;
                                @let cls = if active {
                                    "seg-warm-btn seg-warm-btn-on"
                                } else {
                                    "seg-warm-btn"
                                };
                                button type="button" class=(cls)
                                       onclick={"segmentedSelect(this, 'alts', " (n) ")"} {
                                    (n)
                                }
                            }
                        }
                        input type="hidden" name="alts" value=(knobs.alts);
                        p class="text-xs mt-1 italic" style="color: var(--muted)" { "Extra lineups to compare" }
                    }

                    // Time budget — slider 1-10
                    div {
                        label class="block text-xs font-semibold uppercase tracking-wide mb-1" style="color: var(--ink-2)" {
                            "Time budget"
                        }
                        div class="flex items-center gap-2" {
                            input name="budget" type="range" min="1" max="10"
                                  value=(knobs.budget)
                                  class="range-warm flex-1"
                                  oninput="document.getElementById('budget-val').textContent = this.value + 's'; knobChanged()";
                            span #budget-val class="text-sm font-mono-stat w-8" style="color: var(--ink-2)" {
                                (knobs.budget) "s"
                            }
                        }
                        p class="text-xs mt-1 italic" style="color: var(--muted)" { "Time budget per alternative" }
                    }

                    // Novelty — slider 0-5
                    div {
                        label class="block text-xs font-semibold uppercase tracking-wide mb-1" style="color: var(--ink-2)" {
                            "Novelty"
                        }
                        div class="flex items-center gap-2" {
                            input name="novelty" type="range" min="0" max="5"
                                  value=(knobs.novelty)
                                  class="range-warm flex-1"
                                  oninput="document.getElementById('novelty-val').textContent = this.value === '0' ? 'Off' : this.value; knobChanged()";
                            span #novelty-val class="text-sm font-mono-stat w-8" style="color: var(--ink-2)" {
                                @if knobs.novelty == 0 { "Off" } @else { (knobs.novelty) }
                            }
                        }
                        p class="text-xs mt-1 italic" style="color: var(--muted)" { "Avoid repeating recent lineups" }
                    }
                    input type="hidden" name="generate" value="1";
                    // Carry walk-ons and no-shows through re-solves.
                    @for w in &knobs.walkon {
                        input type="hidden" name="walkon" value=(w);
                    }
                    @for ns in &knobs.no_show {
                        input type="hidden" name="no_show" value=(ns);
                    }
                    // OOB target: editor injects pin state + active boats here.
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
                    div {
                        label class="block text-xs font-semibold uppercase tracking-wide mb-1 invisible" { "\u{00a0}" }
                        div class="flex items-center space-x-3" {
                            button type="submit"
                                   class="btn-accent whitespace-nowrap shadow transition" {
                                (button_label)
                            }
                            span #solve-spinner class="htmx-indicator text-xs" style="color: var(--muted)" {
                                "Generating\u{2026}"
                            }
                        }
                        p class="text-xs mt-1 invisible" { "\u{00a0}" }
                    }
                }
            }
                } // div.px-6
                } // div x-show
            } // div x-data
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

/// Render just the preset bar section for HTMX `outerHTML` swaps.
/// Called by the `GET /solve/{id}/preset-bar` endpoint.
pub(crate) fn preset_bar(
    practice_id: PracticeId,
    knobs: &SolveKnobs,
    custom_profiles: &[(String, Option<String>)],
) -> Markup {
    // Re-use the knobs_form rendering but extract just the preset bar.
    // For now, render the full bar inline.
    html! {
        div #preset-bar class="mb-4" {
            div class="block text-xs font-semibold uppercase tracking-wide mb-2" style="color: var(--ink-2)" {
                "Solver preset"
            }
            (preset_buttons(practice_id, knobs, custom_profiles))
            input type="hidden" name="preset" value=(if knobs.preset.is_empty() { "balanced" } else { &knobs.preset });
        }
    }
}

/// Shared preset button bar used by both the full knobs form and the
/// HTMX partial preset-bar endpoint.
fn preset_buttons(
    practice_id: PracticeId,
    knobs: &SolveKnobs,
    custom_profiles: &[(String, Option<String>)],
) -> Markup {
    let current = &knobs.preset;
    html! {
        div class="seg-warm flex-wrap" {
            @for (value, label) in &[
                ("balanced", "Balanced"),
                ("even_speed", "Even speed"),
                ("tiered", "Tiered"),
                ("random", "Random"),
            ] {
                @let is_active = current == value || (current.is_empty() && *value == "balanced");
                @let btn_class = if is_active {
                    "seg-warm-btn seg-warm-btn-on"
                } else {
                    "seg-warm-btn"
                };
                @let preset_url = preset_url_with(practice_id, knobs, value);
                button type="button" class=(btn_class)
                       hx-get=(preset_url)
                       hx-target="#preset-bar"
                       hx-swap="outerHTML"
                       onclick={"presetClicked('" (label) "')"} {
                    (label)
                }
            }
            @for (name, description) in custom_profiles {
                @let is_active = current == name;
                @let btn_class = if is_active {
                    "seg-warm-btn seg-warm-btn-on"
                } else {
                    "seg-warm-btn"
                };
                @let preset_url = preset_url_with(practice_id, knobs, name);
                button type="button" class=(btn_class)
                       title=[description.as_deref()]
                       style={"color: var(--accent)" }
                       hx-get=(preset_url)
                       hx-target="#preset-bar"
                       hx-swap="outerHTML"
                       onclick={"presetClicked('" (name) "')"} {
                    (name)
                }
            }
            // "+" and gear — static, always visible, no jumping
            @let active_preset = if knobs.preset.is_empty() { "balanced" } else { &knobs.preset };
            @let new_url = format!("/solver-profile/edit?basis={active_preset}");
            @let edit_url = format!("/solver-profile/edit?name={active_preset}");
            button type="button"
                   class="seg-warm-btn"
                   title="Create new preset"
                   style="color: var(--muted)"
                   hx-get=(&new_url)
                   hx-target="body"
                   hx-swap="beforeend" {
                "+"
            }
            button type="button"
                   class="seg-warm-btn"
                   title="View/edit active preset"
                   style="color: var(--muted)"
                   hx-get=(&edit_url)
                   hx-target="body"
                   hx-swap="beforeend" {
                "\u{2699}"
            }
        }
    }
}

fn knob_input(name: &str, label: &str, value: i64, min: Option<i64>, help: Option<&str>) -> Markup {
    html! {
        div {
            label for=(name) class="block text-xs font-semibold uppercase tracking-wide mb-1" style="color: var(--ink-2)" {
                (label)
            }
            input id=(name) name=(name) type="number"
                  value=(value)
                  min=[min.map(|m| m.to_string())]
                  class="w-full rounded px-3 py-2 font-mono-stat text-sm focus:outline-none"
                  style="border: 1px solid var(--rule); background: var(--paper-2); color: var(--ink)";
            @if let Some(h) = help {
                p class="text-xs mt-1 whitespace-nowrap overflow-hidden text-ellipsis italic" style="color: var(--muted)" title=(h) { (h) }
            }
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
