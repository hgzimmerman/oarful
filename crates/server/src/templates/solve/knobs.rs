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
pub(super) fn knobs_form(practice_id: PracticeId, knobs: &SolveKnobs, practices: &[lineup_db::practice::Practice], has_generated: bool, custom_profiles: &[(String, Option<String>)], snapshot: &DbSnapshot, solve_result: Option<&SolveResult>) -> Markup {
    let has_eight = snapshot.boats.iter().any(|b| b.seat_count >= 8);
    let button_label = if has_generated { "Re-generate" } else { "Generate" };
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
        section class="bg-white rounded-lg shadow" {
            // Collapsible knobs — open on landing, collapsed after generation.
            @let preset_label = if knobs.preset.is_empty() { "Balanced" } else { &knobs.preset };
            details open[!has_generated] class="group" {
                summary class="list-none flex items-center justify-between px-6 py-4 cursor-pointer select-none hover:bg-slate-50 transition [&::-webkit-details-marker]:hidden" {
                    div class="flex items-center gap-3 flex-wrap" {
                        h3 class="text-sm font-semibold text-slate-800" { "Solver settings" }
                        span #knob-preset-label class="text-xs text-slate-500" { (preset_label) }
                        @if let Some(result) = solve_result {
                            @if result.status == SolveStatus::Satisfied {
                                @let elapsed_ms = result.elapsed.as_millis();
                                @let elapsed_label = if elapsed_ms < 1000 {
                                    format!("{elapsed_ms}ms")
                                } else {
                                    format!("{:.1}s", result.elapsed.as_secs_f64())
                                };
                                span #knob-metrics class="text-xs text-slate-400" {
                                    "· " (elapsed_label)
                                    @if let Some(obj) = result.objective {
                                        " · obj " (obj)
                                    }
                                }
                            }
                        }
                    }
                    // CSS-only chevron: rotates on open.
                    span class="border-solid border-slate-400 border-r-2 border-b-2 border-t-0 border-l-0 inline-block w-2 h-2 transform rotate-[-45deg] group-open:rotate-45 transition-transform" {}
                }
                div class="px-6 pb-6 border-t border-slate-100 pt-4" {
            form method="get" action=(action)
                 hx-get=(action)
                 hx-target="#solve-results"
                 hx-push-url="true"
                 hx-indicator="#solve-spinner" {

                // Based-on checkbox list + similarity weight
                @if !practices.is_empty() {
                    div class="mb-4" {
                        div class="flex flex-wrap gap-4 items-end" {
                            fieldset class="flex-1" {
                                legend class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-2" {
                                    "Based on"
                                }
                                div class="flex flex-wrap gap-3" {
                                    @for p in practices.iter().take(5) {
                                        @let date_str = p.date.format("%Y-%m-%d").to_string();
                                        @let weekday = p.date.format("%a").to_string();
                                        @let checked = knobs.based_on.contains(&date_str);
                                        label class="inline-flex items-center gap-1.5 text-sm text-slate-700 cursor-pointer" {
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
                    div class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-2" {
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
                        label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                            "Partial fill"
                        }
                        div class="inline-flex rounded border border-slate-300 overflow-hidden text-sm" {
                            @for (val, lbl) in &[(0, "Off"), (1, "1 empty"), (2, "2 empty")] {
                                @let active = knobs.partial == *val;
                                @let cls = if active {
                                    "px-3 py-2 font-semibold bg-slate-800 text-white"
                                } else {
                                    "px-3 py-2 text-slate-700 hover:bg-slate-100"
                                };
                                button type="button" class=(cls)
                                       onclick={"segmentedSelect(this, 'partial', " (val) ")"} {
                                    (lbl)
                                }
                            }
                        }
                        input type="hidden" name="partial" value=(knobs.partial);
                        p class="text-xs text-slate-500 mt-1" { "Empty optional seats per boat" }
                    }
                    }

                    // Alternatives — segmented (0 / 1 / 2 / 3)
                    div {
                        label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                            "Alternatives"
                        }
                        div class="inline-flex rounded border border-slate-300 overflow-hidden text-sm" {
                            @for n in 0..=3i64 {
                                @let active = knobs.alts as i64 == n;
                                @let cls = if active {
                                    "px-3 py-2 font-semibold bg-slate-800 text-white"
                                } else {
                                    "px-3 py-2 text-slate-700 hover:bg-slate-100"
                                };
                                button type="button" class=(cls)
                                       onclick={"segmentedSelect(this, 'alts', " (n) ")"} {
                                    (n)
                                }
                            }
                        }
                        input type="hidden" name="alts" value=(knobs.alts);
                        p class="text-xs text-slate-500 mt-1" { "Extra lineups to compare" }
                    }

                    // Time budget — slider 1-10
                    div {
                        label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                            "Time budget"
                        }
                        div class="flex items-center gap-2" {
                            input name="budget" type="range" min="1" max="10"
                                  value=(knobs.budget)
                                  class="flex-1 accent-blue-600"
                                  oninput="document.getElementById('budget-val').textContent = this.value + 's'; knobChanged()";
                            span #budget-val class="text-sm font-mono text-slate-700 w-8" {
                                (knobs.budget) "s"
                            }
                        }
                        p class="text-xs text-slate-500 mt-1" { "Per-alternative solve cap" }
                    }

                    // Novelty — slider 0-5
                    div {
                        label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                            "Novelty"
                        }
                        div class="flex items-center gap-2" {
                            input name="novelty" type="range" min="0" max="5"
                                  value=(knobs.novelty)
                                  class="flex-1 accent-blue-600"
                                  oninput="document.getElementById('novelty-val').textContent = this.value === '0' ? 'Off' : this.value; knobChanged()";
                            span #novelty-val class="text-sm font-mono text-slate-700 w-8" {
                                @if knobs.novelty == 0 { "Off" } @else { (knobs.novelty) }
                            }
                        }
                        p class="text-xs text-slate-500 mt-1" { "Avoid repeating recent lineups" }
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
                        label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1 invisible" { "\u{00a0}" }
                        div class="flex items-center space-x-3" {
                            button type="submit"
                                   class="whitespace-nowrap bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition" {
                                (button_label)
                            }
                            span #solve-spinner class="htmx-indicator text-xs text-slate-500" {
                                "Generating…"
                            }
                        }
                        p class="text-xs mt-1 invisible" { "\u{00a0}" }
                    }
                }
            }
                } // div.px-6 (details body)
            } // details
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
            div class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-2" {
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
        div class="inline-flex rounded-lg border border-slate-300 overflow-hidden text-sm flex-wrap" {
            @for (value, label) in &[
                ("balanced", "Balanced"),
                ("even_speed", "Even speed"),
                ("tiered", "Tiered"),
                ("random", "Random"),
            ] {
                @let is_active = current == value || (current.is_empty() && *value == "balanced");
                @let btn_class = if is_active {
                    "px-3 py-2 font-semibold bg-slate-800 text-white"
                } else {
                    "px-3 py-2 text-slate-700 hover:bg-slate-100"
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
                    "px-3 py-2 font-semibold bg-violet-700 text-white"
                } else {
                    "px-3 py-2 text-violet-700 hover:bg-violet-50"
                };
                @let preset_url = preset_url_with(practice_id, knobs, name);
                button type="button" class=(btn_class)
                       title=[description.as_deref()]
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
                   class="px-3 py-2 text-slate-400 hover:text-slate-700 hover:bg-slate-100"
                   title="Create new preset"
                   hx-get=(&new_url)
                   hx-target="body"
                   hx-swap="beforeend" {
                "+"
            }
            button type="button"
                   class="px-3 py-2 text-slate-400 hover:text-slate-700 hover:bg-slate-100"
                   title="View/edit active preset"
                   hx-get=(&edit_url)
                   hx-target="body"
                   hx-swap="beforeend" {
                "\u{2699}"
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
                p class="text-xs text-slate-500 mt-1 whitespace-nowrap overflow-hidden text-ellipsis" title=(h) { (h) }
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
            div class="bg-red-50 border-l-4 border-red-500 px-4 py-3 rounded text-sm text-red-900" {
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
            div class="bg-amber-50 border-l-4 border-amber-500 px-4 py-3 rounded text-sm text-amber-900" {
                strong { "No result." }
                " Solver timed out without finding any valid lineup. Try increasing the time budget or relaxing constraints."
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
            format!(
                "Seat lock skipped: {rower_name} in seat {seat} on {boat_name} — {reason}."
            )
        }
    }
}
