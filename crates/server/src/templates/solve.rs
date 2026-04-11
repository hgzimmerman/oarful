//! Solve view template. Renders the solver's primary lineup, its
//! alternatives (Alpine-toggled), the unplaced-rowers breakdown, and
//! the commit button.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use lineup_db::{
    boat::{types::BoatId, Boat},
    practice::Practice,
    rower::{types::RowerId, Rower},
    snapshot::DbSnapshot,
};
use lineup_solver::{
    Diagnostic, ProposedLineup, ProposedSolution, SolveResult, SolveStatus, UnplacedRowers,
};
use maud::{html, Markup};

use super::layout::page_header;
use crate::handlers::solve::SolveKnobs;

/// Display flags threaded through lineup card rendering.
#[derive(Clone)]
pub(crate) struct DisplayFlags {
    pub(crate) show_attributes: bool,
    pub(crate) force_cox_stern: bool,
    /// Locked (rower_id, boat_id, seat) triples. Used to render lock
    /// icons and distinct styling on locked seats.
    pub(crate) locked_seats: HashSet<(RowerId, BoatId, i32)>,
}

/// Landing page before the solver runs. Shows knobs with a
/// "Generate" button (or "Re-generate" if lineups already exist),
/// plus a manual lineup builder with boat selection and an
/// available rower pool.
pub(crate) fn landing_content(
    snapshot: &DbSnapshot,
    date: NaiveDate,
    knobs: &SolveKnobs,
    committed_practices: &[Practice],
    has_committed: bool,
    custom_profiles: &[(String, Option<String>)],
) -> Markup {
    let available_count = snapshot.available_rowers().count();
    let subtitle = format!(
        "{available_count} members available · {boats} candidate shells",
        boats = snapshot.sweep_boats.len(),
    );

    // Roster members not currently available (candidates for walk-on).
    let unavailable: Vec<&Rower> = snapshot
        .rowers
        .iter()
        .filter(|r| r.active.as_bool())
        .filter(|r| {
            !snapshot
                .availability
                .get(&r.id)
                .map(|s| s.is_available_for_sweep())
                .unwrap_or(false)
        })
        .collect();

    html! {
        (page_header(&format!("Generate · {date}"), Some(&subtitle)))
        div class="px-8 py-6 space-y-6 max-w-6xl" {
            (knobs_form(date, knobs, committed_practices, has_committed, custom_profiles))
            (walkon_section(date, &unavailable, knobs))
            (manual_builder(snapshot, date))
        }
    }
}

/// "+ Add walk-on" section: a dropdown of unavailable roster members.
/// Selecting one adds a `walkon` param and reloads the page so the
/// rower appears in the available pool.
fn walkon_section(date: NaiveDate, unavailable: &[&Rower], knobs: &SolveKnobs) -> Markup {
    if unavailable.is_empty() && knobs.walkon.is_empty() {
        return html! {};
    }
    let action = format!("/solve/{date}");
    html! {
        section class="bg-white rounded-lg shadow p-4" {
            div class="flex items-end gap-3 flex-wrap" {
                // Already-added walk-ons shown as pills
                @if !knobs.walkon.is_empty() {
                    div class="flex flex-wrap gap-1.5 items-center mr-2" {
                        span class="text-xs font-semibold text-slate-700 uppercase tracking-wide" { "Walk-ons:" }
                        @for id_str in &knobs.walkon {
                            span class="inline-block px-2 py-0.5 text-xs bg-emerald-100 text-emerald-800 rounded-full" {
                                @if let Ok(id) = id_str.parse::<lineup_db::rower::types::RowerId>() {
                                    @if let Some(r) = unavailable.iter().find(|r| r.id == id) {
                                        (r.name)
                                    } @else {
                                        "#" (id_str)
                                    }
                                } @else {
                                    (id_str)
                                }
                            }
                        }
                    }
                }

                // Add walk-on dropdown
                @if !unavailable.is_empty() {
                    form method="get" action=(action)
                         hx-get=(action)
                         hx-target="#content"
                         hx-push-url="true"
                         class="flex items-end gap-2" {
                        // Carry existing knobs through
                        input type="hidden" name="partial" value=(knobs.partial);
                        input type="hidden" name="novelty" value=(knobs.novelty);
                        input type="hidden" name="alts" value=(knobs.alts);
                        input type="hidden" name="budget" value=(knobs.budget);
                        @if !knobs.preset.is_empty() {
                            input type="hidden" name="preset" value=(&knobs.preset);
                        }
                        @for l in &knobs.lock {
                            input type="hidden" name="lock" value=(l);
                        }
                        @for w in &knobs.walkon {
                            input type="hidden" name="walkon" value=(w);
                        }
                        div {
                            label for="new-walkon" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                                "+ Add walk-on"
                            }
                            select id="new-walkon" name="walkon"
                                   class="border border-slate-300 rounded px-2 py-1.5 text-sm focus:border-slate-500 focus:outline-none" {
                                @for r in unavailable {
                                    // Skip rowers already added as walk-ons.
                                    @if !knobs.walkon.contains(&r.id.as_int().to_string()) {
                                        option value=(r.id) { (r.name) }
                                    }
                                }
                            }
                        }
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold px-3 py-1.5 rounded shadow transition" {
                            "Add"
                        }
                    }
                }
            }
        }
    }
}

/// Manual lineup builder: boat selector + empty boat cards + rower pool.
/// The coach can place rowers by hand and either commit directly or
/// click Generate to let the solver fill the rest (placements become locks).
fn manual_builder(snapshot: &DbSnapshot, date: NaiveDate) -> Markup {
    let commit_action = format!("/commit-lineup/{date}");
    let available_rowers: Vec<&Rower> = snapshot.available_rowers().collect();

    html! {
        section class="bg-white rounded-lg shadow p-6"
               x-data="manualBuilder()" {
            div class="flex items-center justify-between mb-4" {
                h2 class="text-xl font-bold text-slate-800" { "Manual lineup" }
                div class="flex items-center gap-2" {
                    template x-if="selected" {
                        span class="text-xs text-blue-600" {
                            "Click a seat to place, or click again to cancel"
                        }
                    }
                    form method="post" action=(commit_action) x-ref="manualCommitForm" {
                        div x-ref="manualSeatInputs" {}
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition" {
                            "Commit lineup"
                        }
                    }
                }
            }

            // Boat selector
            div class="mb-4" {
                div class="text-xs font-semibold text-slate-700 uppercase tracking-wide mb-2" {
                    "Select boats"
                }
                div class="flex flex-wrap gap-3" {
                    @for boat in &snapshot.sweep_boats {
                        @let bid = boat.id.as_int().to_string();
                        label class="inline-flex items-center gap-1.5 text-sm text-slate-700 cursor-pointer" {
                            input type="checkbox"
                                  value=(bid)
                                  class="rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                                  "@change"={"toggleBoat(" (boat.id.as_int()) ")"};
                            (boat.name) " (" (boat.seat_count)
                            @if boat.has_cox.as_bool() { "+" }
                            ")"
                        }
                    }
                }
            }

            // Empty boat cards (shown when boats are selected)
            div #manual-boats class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4" {}

            // Available rower pool
            div class="pt-4 border-t border-slate-200 text-sm" {
                div class="mb-2" {
                    strong class="text-slate-700" { "Available members " }
                    span class="text-xs text-slate-500" { "(click to select, then click a seat)" }
                }
                div #rower-pool class="flex flex-wrap gap-2" {
                    @for r in &available_rowers {
                        @let key = format!("pool:{}", r.id);
                        span data-key=(key)
                             data-rower=(r.id)
                             data-name=(r.name)
                             class="inline-block px-3 py-1.5 rounded border border-slate-200 cursor-pointer transition hover:bg-slate-50"
                             ":class"={"selected === '" (key) "' ? 'bg-blue-100 ring-2 ring-blue-400 border-blue-400' : 'hover:bg-slate-50'"}
                             "@click"={"selectRower('" (key) "')"} {
                            div class="font-medium text-slate-800 text-sm" { (r.name) }
                        }
                    }
                }
            }
        }

        script {
            (maud::PreEscaped(manual_builder_js(snapshot)))
        }
    }
}

/// Generate the Alpine component JS for the manual builder.
fn manual_builder_js(snapshot: &DbSnapshot) -> String {
    // Build a JS object mapping boat_id → { name, seat_count, has_cox }
    let mut boats_js = String::from("{");
    for (i, boat) in snapshot.sweep_boats.iter().enumerate() {
        if i > 0 { boats_js.push(','); }
        let escaped_name = boat.name
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        boats_js.push_str(&format!(
            "{}:{{name:\"{}\",seats:{},hasCox:{}}}",
            boat.id.as_int(),
            escaped_name,
            boat.seat_count,
            boat.has_cox.as_bool(),
        ));
    }
    boats_js.push('}');

    format!(r#"
function manualBuilder() {{
    var boatDefs = {boats_js};
    return {{
        selected: null,
        selectedBoats: {{}},
        init() {{ this.rebuildInputs(); }},
        toggleBoat(boatId) {{
            if (this.selectedBoats[boatId]) {{
                delete this.selectedBoats[boatId];
            }} else {{
                this.selectedBoats[boatId] = true;
            }}
            this.renderBoats();
            this.rebuildInputs();
        }},
        selectRower(key) {{
            if (!this.selected) {{
                this.selected = key;
            }} else if (this.selected === key) {{
                this.selected = null;
            }} else {{
                // If first selection was a seat, place this rower there
                this.placeRower(this.selected, key);
                this.selected = null;
            }}
        }},
        selectSeat(key) {{
            if (!this.selected) {{
                this.selected = key;
            }} else if (this.selected === key) {{
                this.selected = null;
            }} else {{
                // If first selection was a rower, place them in this seat
                this.placeRower(key, this.selected);
                this.selected = null;
            }}
        }},
        placeRower(seatKey, rowerKey) {{
            var seatEl = this.$root.querySelector('[data-seat-key="' + seatKey + '"]');
            var rowerEl = this.$root.querySelector('[data-key="' + rowerKey + '"]');
            if (!seatEl || !rowerEl) return;
            var rowerId = rowerEl.dataset.rower;
            var rowerName = rowerEl.dataset.name;
            if (!rowerId) return;
            // Place rower in seat
            seatEl.dataset.rower = rowerId;
            seatEl.querySelector('.seat-content').innerHTML =
                '<div class="font-medium text-slate-800">' + this.esc(rowerName) + '</div>';
            seatEl.classList.remove('text-slate-400');
            seatEl.classList.add('text-slate-800');
            // Remove from pool
            rowerEl.style.display = 'none';
            this.rebuildInputs();
        }},
        clearSeat(seatKey) {{
            var seatEl = this.$root.querySelector('[data-seat-key="' + seatKey + '"]');
            if (!seatEl || !seatEl.dataset.rower) return;
            var rowerId = seatEl.dataset.rower;
            // Return to pool
            var poolEl = this.$root.querySelector('[data-key="pool:' + rowerId + '"]');
            if (poolEl) poolEl.style.display = '';
            // Clear seat
            seatEl.dataset.rower = '';
            var label = seatEl.dataset.seatLabel || '';
            seatEl.querySelector('.seat-content').innerHTML =
                '<span class="italic">\u2014 empty \u2014</span>';
            seatEl.classList.add('text-slate-400');
            seatEl.classList.remove('text-slate-800');
            this.rebuildInputs();
        }},
        renderBoats() {{
            var container = this.$root.querySelector('#manual-boats');
            container.innerHTML = '';
            var self = this;
            Object.keys(this.selectedBoats).forEach(function(bid) {{
                var def = boatDefs[bid];
                if (!def) return;
                var card = document.createElement('div');
                card.className = 'border border-slate-200 rounded-lg overflow-hidden';
                var header = '<div class="bg-slate-100 px-4 py-2 border-b border-slate-200">' +
                    '<strong class="text-slate-800">' + self.esc(def.name) + '</strong>' +
                    '<span class="text-xs text-slate-500 ml-2">(' + def.seats + (def.hasCox ? '+' : '') + ')</span></div>';
                var rows = '';
                var seats = [];
                if (def.hasCox) seats.push(0);
                for (var s = def.seats; s >= 1; s--) seats.push(s);
                seats.forEach(function(s) {{
                    var seatKey = bid + ':' + s;
                    var label = s === 0 ? 'cox' : 's' + s;
                    rows += '<tr data-seat-key="' + seatKey + '" data-boat="' + bid + '" data-seat="' + s + '" data-rower="" ' +
                        'class="border-b border-slate-100 last:border-0 cursor-pointer transition text-slate-400 hover:bg-slate-50" ' +
                        '@click="selectSeat(\'' + seatKey + '\')" ' +
                        ':class="selected === \'' + seatKey + '\' ? \'bg-blue-100 ring-2 ring-inset ring-blue-400\' : \'hover:bg-slate-50\'">' +
                        '<td class="px-4 py-2 text-slate-500 font-mono text-xs w-12">' + label + '</td>' +
                        '<td class="px-4 py-2 seat-content"><span class="italic">\u2014 empty \u2014</span></td>' +
                        '<td class="w-8 text-center"><button type="button" class="text-xs text-slate-400 hover:text-red-600" ' +
                        '@click.stop="clearSeat(\'' + seatKey + '\')" title="Clear seat">\u00d7</button></td></tr>';
                }});
                card.innerHTML = header + '<table class="w-full text-sm"><tbody>' + rows + '</tbody></table>';
                container.appendChild(card);
            }});
        }},
        esc(s) {{
            var d = document.createElement('div');
            d.textContent = s;
            return d.innerHTML;
        }},
        rebuildInputs() {{
            // Rebuild commit form hidden inputs.
            var container = this.$refs.manualSeatInputs;
            if (!container) return;
            container.innerHTML = '';
            var placements = [];
            this.$root.querySelectorAll('[data-seat-key][data-boat][data-seat][data-rower]').forEach(function(el) {{
                if (!el.dataset.rower || el.dataset.rower === '') return;
                var val = el.dataset.boat + ':' + el.dataset.seat + ':' + el.dataset.rower;
                var inp = document.createElement('input');
                inp.type = 'hidden';
                inp.name = 'seat';
                inp.value = val;
                container.appendChild(inp);
                placements.push(el.dataset.rower + ':' + el.dataset.boat + ':' + el.dataset.seat);
            }});
            // Also inject placements as locks into the knobs form so
            // Generate treats them as seat locks.
            var knobsForm = document.querySelector('form[hx-get]');
            if (knobsForm) {{
                knobsForm.querySelectorAll('input[name="lock"].manual-lock').forEach(function(el) {{ el.remove(); }});
                placements.forEach(function(lockVal) {{
                    var inp = document.createElement('input');
                    inp.type = 'hidden';
                    inp.name = 'lock';
                    inp.value = lockVal;
                    inp.className = 'manual-lock';
                    knobsForm.appendChild(inp);
                }});
            }}
        }}
    }};
}}
"#)
}

pub(crate) fn view_content(
    snapshot: &DbSnapshot,
    date: NaiveDate,
    knobs: &SolveKnobs,
    result: &SolveResult,
    committed_practices: &[Practice],
    flags: &DisplayFlags,
    custom_profiles: &[(String, Option<String>)],
) -> Markup {
    let available = snapshot.available_rowers().count();
    let subtitle = format!(
        "{available} members available · {boats} candidate shells",
        boats = snapshot.sweep_boats.len(),
    );

    html! {
        (page_header(&format!("Generate · {date}"), Some(&subtitle)))
        div class="px-8 py-6 space-y-6 max-w-6xl" {
            (knobs_form(date, knobs, committed_practices, true, custom_profiles))
            (status_banner(date, &result.status, &result.diagnostics))

            @if result.status == SolveStatus::Satisfied {
                (primary_panel(snapshot, date, knobs, &result.primary, flags))

                @if !result.alternatives.is_empty() {
                    (alternatives_panel(snapshot, &result.primary, &result.alternatives, flags))
                }
            }
        }
    }
}

/// Coach-tunable knobs (partial fill / novelty / alternatives / time
/// budget). Submitting hx-gets the same `/solve/{date}` URL with the
/// new query string, so the result is bookmarkable and the back
/// button works.
fn knobs_form(date: NaiveDate, knobs: &SolveKnobs, practices: &[Practice], has_generated: bool, custom_profiles: &[(String, Option<String>)]) -> Markup {
    let button_label = if has_generated { "Re-generate" } else { "Generate" };
    let action = format!("/solve/{date}");
    html! {
        section class="bg-white rounded-lg shadow p-6" {
            form method="get" action=(action)
                 hx-get=(action)
                 hx-target="#content"
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
                                    @for p in practices.iter().take(8) {
                                        @let date_str = p.date.format("%Y-%m-%d").to_string();
                                        @let weekday = p.date.format("%a").to_string();
                                        @let checked = knobs.based_on.contains(&date_str);
                                        label class="inline-flex items-center gap-1.5 text-sm text-slate-700 cursor-pointer" {
                                            input type="checkbox" name="based_on" value=(date_str)
                                                  checked[checked]
                                                  class="rounded border-slate-300 text-blue-600 focus:ring-blue-500";
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
                    div class="inline-flex rounded-lg border border-slate-300 overflow-hidden text-sm flex-wrap" {
                        @let current = &knobs.preset;
                        @for (value, label, tip) in &[
                            ("balanced", "Balanced", "Even-handed defaults — no emphasis on speed parity or stacking"),
                            ("even_speed", "Even speed", "Boats matched in speed — talent spread evenly, flexible side placement"),
                            ("tiered", "Tiered", "Top boat stacked — best rowers in key seats, skill gaps between boats OK"),
                            ("random", "Random", "No soft preferences — only hard constraints, maximum variety"),
                        ] {
                            @let is_active = current == value || (current.is_empty() && *value == "balanced");
                            @let btn_class = if is_active {
                                "px-3 py-1.5 font-semibold bg-slate-800 text-white"
                            } else {
                                "px-3 py-1.5 text-slate-700 hover:bg-slate-100"
                            };
                            @let preset_url = preset_url_with(date, knobs, value);
                            button type="button" class=(btn_class)
                                   title=(tip)
                                   hx-get=(preset_url)
                                   hx-target="#preset-bar"
                                   hx-swap="outerHTML" {
                                (label)
                            }
                        }
                        @for (name, description) in custom_profiles {
                            @let is_active = current == name;
                            @let btn_class = if is_active {
                                "px-3 py-1.5 font-semibold bg-violet-700 text-white"
                            } else {
                                "px-3 py-1.5 text-violet-700 hover:bg-violet-50"
                            };
                            @let delete_url = format!("/solver-profile/{}", name);
                            @let preset_url = preset_url_with(date, knobs, name);
                            span class="relative inline-flex items-center" {
                                button type="button" class=(btn_class)
                                       title=[description.as_deref()]
                                       hx-get=(preset_url)
                                       hx-target="#preset-bar"
                                       hx-swap="outerHTML" {
                                    (name)
                                }
                                button type="button"
                                       class="text-xs text-violet-400 hover:text-red-600 ml-0.5 -mr-1"
                                       title="Delete this profile"
                                       hx-delete=(delete_url)
                                       hx-confirm={"Delete profile \"" (name) "\"?"}
                                       hx-target="#content"
                                       hx-swap="none"
                                       onclick={"event.stopPropagation(); setTimeout(()=>location.reload(), 200)"} {
                                    "×"
                                }
                            }
                        }
                    }
                    input type="hidden" name="preset" value=(if knobs.preset.is_empty() { "balanced" } else { &knobs.preset });
                }

                // Existing knobs grid
                div class="grid grid-cols-2 md:grid-cols-5 gap-4 items-end" {
                    (knob_input(
                        "partial",
                        "Partial fill",
                        knobs.partial as i64,
                        Some(0),
                        Some("0 = strict; N = empty seats allowed"),
                    ))
                    (knob_input(
                        "novelty",
                        "Novelty",
                        knobs.novelty as i64,
                        Some(0),
                        Some("Avoidance weight; 0 disables"),
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
                    input type="hidden" name="generate" value="1";
                    // Carry seat locks and walk-ons through re-solves.
                    @for l in &knobs.lock {
                        input type="hidden" name="lock" value=(l);
                    }
                    @for w in &knobs.walkon {
                        input type="hidden" name="walkon" value=(w);
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
        }
    }
}

/// Build a URL that switches the preset while carrying all other knobs.
fn preset_url_with(date: NaiveDate, knobs: &SolveKnobs, new_preset: &str) -> String {
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
    for w in &knobs.walkon {
        parts.push(format!("walkon={w}"));
    }
    format!("/solve/{date}/preset-bar?{}", parts.join("&"))
}

/// Render just the preset bar section for HTMX `outerHTML` swaps.
/// Called by the `GET /solve/{date}/preset-bar` endpoint.
pub(crate) fn preset_bar(
    date: NaiveDate,
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
            div class="inline-flex rounded-lg border border-slate-300 overflow-hidden text-sm flex-wrap" {
                @let current = &knobs.preset;
                @for (value, label, tip) in &[
                    ("balanced", "Balanced", "Even-handed defaults — no emphasis on speed parity or stacking"),
                    ("even_speed", "Even speed", "Boats matched in speed — talent spread evenly, flexible side placement"),
                    ("tiered", "Tiered", "Top boat stacked — best rowers in key seats, skill gaps between boats OK"),
                    ("random", "Random", "No soft preferences — only hard constraints, maximum variety"),
                ] {
                    @let is_active = current == value || (current.is_empty() && *value == "balanced");
                    @let btn_class = if is_active {
                        "px-3 py-1.5 font-semibold bg-slate-800 text-white"
                    } else {
                        "px-3 py-1.5 text-slate-700 hover:bg-slate-100"
                    };
                    @let preset_url = preset_url_with(date, knobs, value);
                    button type="button" class=(btn_class)
                           title=(tip)
                           hx-get=(preset_url)
                           hx-target="#preset-bar"
                           hx-swap="outerHTML" {
                        (label)
                    }
                }
                @for (name, description) in custom_profiles {
                    @let is_active = current == name;
                    @let btn_class = if is_active {
                        "px-3 py-1.5 font-semibold bg-violet-700 text-white"
                    } else {
                        "px-3 py-1.5 text-violet-700 hover:bg-violet-50"
                    };
                    @let delete_url = format!("/solver-profile/{}", name);
                    @let preset_url = preset_url_with(date, knobs, name);
                    span class="relative inline-flex items-center" {
                        button type="button" class=(btn_class)
                               title=[description.as_deref()]
                               hx-get=(preset_url)
                               hx-target="#preset-bar"
                               hx-swap="outerHTML" {
                            (name)
                        }
                        button type="button"
                               class="text-xs text-violet-400 hover:text-red-600 ml-0.5 -mr-1"
                               title="Delete this profile"
                               hx-delete=(delete_url)
                               hx-confirm={"Delete profile \"" (name) "\"?"}
                               hx-swap="none"
                               onclick={"event.stopPropagation(); setTimeout(()=>location.reload(), 200)"} {
                            "×"
                        }
                    }
                }
            }
            input type="hidden" name="preset" value=(if knobs.preset.is_empty() { "balanced" } else { &knobs.preset });
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

fn primary_panel(
    snapshot: &DbSnapshot,
    date: NaiveDate,
    _knobs: &SolveKnobs,
    primary: &ProposedSolution,
    flags: &DisplayFlags,
) -> Markup {
    let used: Vec<&ProposedLineup> = primary.lineups.iter().filter(|l| l.used).collect();
    let skipped: Vec<&ProposedLineup> =
        primary.lineups.iter().filter(|l| !l.used).collect();
    let commit_action = format!("/commit-lineup/{date}");

    html! {
        section class="bg-white rounded-lg shadow p-6"
               x-data="swapLineup()" {

            div class="flex items-center justify-between mb-4" {
                h2 class="text-xl font-bold text-slate-800" { "Primary lineup" }
                div class="flex items-center gap-2" {
                    // Hint shown when a rower is selected.
                    template x-if="selected" {
                        span class="text-xs text-blue-600" {
                            "Click another rower to swap"
                            // Bench button — only for seated rowers
                            " · "
                            button type="button"
                                   class="underline text-amber-600 hover:text-amber-800"
                                   "@click"="toBench(selected)" {
                                "move to bench"
                            }
                            " · or click again to cancel"
                        }
                    }
                    form method="post" action=(commit_action) x-ref="commitForm" {
                        // Hidden inputs populated by Alpine from its
                        // seats state. The x-effect rebuilds them
                        // whenever seats changes.
                        div x-ref="seatInputs" {}
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition" {
                            "Commit lineup"
                        }
                    }
                }
            }

            @if used.is_empty() {
                div class="text-slate-500 italic" { "No boats fielded." }
            } @else {
                div class="grid grid-cols-1 md:grid-cols-2 gap-4" {
                    @for lineup in &used {
                        (swap_boat_card(snapshot, lineup, flags))
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

            // Bench / sculling swap targets
            (swap_unplaced_block(snapshot, &primary.unplaced, flags))
        }

        // Alpine swap logic — kept as a separate script block so it's
        // not inlined into every data attribute.
        script {
            (maud::PreEscaped(r#"
function swapLineup() {
    return {
        selected: null,
        init() { this.rebuildInputs(); },
        select(key) {
            if (!this.selected) {
                this.selected = key;
            } else if (this.selected === key) {
                this.selected = null;
            } else {
                this.doSwap(this.selected, key);
                this.selected = null;
            }
        },
        doSwap(a, b) {
            var elA = this.$root.querySelector('[data-key="' + a + '"]');
            var elB = this.$root.querySelector('[data-key="' + b + '"]');
            if (!elA || !elB) return;
            // Swap rower identity (id + name) between the two slots.
            var tmpRower = elA.dataset.rower;
            var tmpName = elA.dataset.name;
            elA.dataset.rower = elB.dataset.rower;
            elA.dataset.name = elB.dataset.name;
            elB.dataset.rower = tmpRower;
            elB.dataset.name = tmpName;
            // Re-render the visible content for both slots.
            this.renderSlot(elA);
            this.renderSlot(elB);
            this.rebuildInputs();
        },
        /** Pull a seated rower to the bench, leaving an empty seat. */
        toBench(key) {
            var el = this.$root.querySelector('[data-key="' + key + '"]');
            if (!el || el.dataset.boat === 'bench' || el.dataset.boat === 'sculling') return;
            var rowerId = el.dataset.rower;
            var rowerName = el.dataset.name;
            if (!rowerId) return;
            // Clear the seat
            el.dataset.rower = '';
            el.dataset.name = '';
            this.renderSlot(el);
            // Add a new bench pill
            this.addBenchPill(rowerId, rowerName);
            this.selected = null;
            this.rebuildInputs();
        },
        /** Add a rower pill to the bench area. */
        addBenchPill(rowerId, rowerName) {
            var container = this.$root.querySelector('#bench-pills');
            if (!container) return;
            var key = 'bench:' + rowerId;
            var span = document.createElement('span');
            span.dataset.key = key;
            span.dataset.boat = 'bench';
            span.dataset.seat = '-1';
            span.dataset.rower = rowerId;
            span.dataset.name = rowerName || '';
            span.className = 'inline-block px-3 py-1.5 rounded border border-slate-200 cursor-pointer transition rower-content hover:bg-slate-50';
            span.setAttribute('@click', "select('" + key + "')");
            span.setAttribute(':class', "selected === '" + key + "' ? 'bg-blue-100 ring-2 ring-blue-400 border-blue-400' : 'hover:bg-slate-50'");
            span.innerHTML = '<div class="font-medium text-slate-800 text-sm">' + this.esc(rowerName || '#' + rowerId) + '</div>';
            container.appendChild(span);
        },
        /** Re-render the visible content of a slot from its data attributes. */
        renderSlot(el) {
            var rowerId = el.dataset.rower;
            var rowerName = el.dataset.name;
            var content = el.querySelector('.rower-content');
            // Bench/sculling pills: the element itself is the content container.
            if (!content) content = el.tagName === 'SPAN' ? el : null;
            if (!content) return;
            if (!rowerId) {
                // Empty seat
                content.innerHTML = '<span class="text-slate-400 italic">\u2014 empty \u2014</span>';
            } else {
                content.innerHTML = '<div class="font-medium text-slate-800' +
                    (el.tagName === 'SPAN' ? ' text-sm' : '') + '">' +
                    this.esc(rowerName || '#' + rowerId) + '</div>';
            }
        },
        esc(s) {
            var d = document.createElement('div');
            d.textContent = s;
            return d.innerHTML;
        },
        rebuildInputs() {
            var container = this.$refs.seatInputs;
            container.innerHTML = '';
            this.$root.querySelectorAll('[data-boat][data-seat][data-rower]').forEach(function(el) {
                if (el.dataset.boat === 'bench' || el.dataset.boat === 'sculling') return;
                if (!el.dataset.rower) return; // empty seat
                var inp = document.createElement('input');
                inp.type = 'hidden';
                inp.name = 'seat';
                inp.value = el.dataset.boat + ':' + el.dataset.seat + ':' + el.dataset.rower;
                container.appendChild(inp);
            });
        },
        /** Toggle a seat lock and re-solve. Finds the knobs form and
         *  adds/removes a hidden input for the lock, then submits. */
        /** Toggle a seat lock. Updates the hidden input in the knobs
         *  form and the visual state of the seat row. Does NOT re-solve
         *  — the lock takes effect on the next Generate click. */
        toggleLock(lockVal) {
            var form = document.querySelector('form[hx-get]');
            if (!form) return;
            var existing = form.querySelector('input[name="lock"][value="' + lockVal + '"]');
            var btn = this.$root.querySelector('[data-lock="' + lockVal + '"]');
            var row = btn ? btn.closest('tr') : null;
            if (existing) {
                existing.remove();
                if (btn) btn.textContent = '\uD83D\uDD13'; // 🔓
                if (row) {
                    row.classList.remove('bg-violet-50', 'border-l-4', 'border-l-violet-400');
                }
            } else {
                var inp = document.createElement('input');
                inp.type = 'hidden';
                inp.name = 'lock';
                inp.value = lockVal;
                form.appendChild(inp);
                if (btn) btn.textContent = '\uD83D\uDD12'; // 🔒
                if (row) {
                    row.classList.add('bg-violet-50', 'border-l-4', 'border-l-violet-400');
                }
            }
        }
    };
}
"#))
        }
    }
}

/// Boat card with clickable seat rows for the swap component.
fn swap_boat_card(snapshot: &DbSnapshot, lineup: &ProposedLineup, flags: &DisplayFlags) -> Markup {
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
                        @let key = format!("{}:{}", lineup.boat_id, seat);
                        @let label = seat_label(*seat, seat_count);
                        @let rower = find_rower(snapshot, *rower_id);
                        @let is_designated_cox = rower.map(|r| r.is_designated_cox.as_bool()).unwrap_or(false);
                        @let is_locked = flags.locked_seats.contains(&(*rower_id, lineup.boat_id, *seat));
                        @let row_base = if is_locked {
                            "border-b border-slate-100 last:border-0 cursor-pointer transition bg-violet-50 border-l-4 border-l-violet-400"
                        } else if is_designated_cox {
                            "border-b border-slate-100 last:border-0 cursor-pointer transition border-l-4 border-l-indigo-400"
                        } else {
                            "border-b border-slate-100 last:border-0 cursor-pointer transition"
                        };
                        @let rower_name = rower.map(|r| r.name.as_str()).unwrap_or("");
                        @let lock_val = format!("{}:{}:{}", rower_id, lineup.boat_id, seat);
                        tr data-key=(key)
                           data-boat=(lineup.boat_id)
                           data-seat=(seat)
                           data-rower=(rower_id)
                           data-name=(rower_name)
                           class=(row_base)
                           ":class"={"selected === '" (key) "' ? 'bg-blue-100 ring-2 ring-inset ring-blue-400' : 'hover:bg-slate-50'"}
                           "@click"={"select('" (key) "')"} {
                            td class="px-4 py-2 w-12" {
                                (seat_badge(boat, *seat, &label))
                            }
                            td class="px-4 py-2 rower-content" {
                                @if let Some(r) = rower {
                                    div class="font-medium text-slate-800" { (r.name) }
                                    (rower_stats_line(r, flags.show_attributes))
                                } @else {
                                    span class="text-slate-400 italic" { "unknown rower #" (rower_id) }
                                }
                            }
                            td class="w-8 text-center" {
                                button type="button"
                                       class="text-xs hover:text-violet-700"
                                       title=(if is_locked { "Unlock seat" } else { "Lock seat" })
                                       data-lock=(lock_val)
                                       "@click.stop"="toggleLock($event.currentTarget.dataset.lock)" {
                                    @if is_locked { "🔒" } @else { "🔓" }
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

/// Bench / sculling rowers as clickable swap targets. The bench
/// container is always rendered so `toBench` can add pills to it.
fn swap_unplaced_block(snapshot: &DbSnapshot, unplaced: &UnplacedRowers, flags: &DisplayFlags) -> Markup {
    html! {
        div class="mt-4 pt-4 border-t border-slate-200 text-sm space-y-2" {
            @if !unplaced.to_sculling.is_empty() {
                div {
                    strong class="text-slate-700" { "To sculling " }
                    span class="text-xs text-slate-500" { "(click to swap into a seat)" }
                }
                div class="flex flex-wrap gap-2 mt-1" {
                    @for id in &unplaced.to_sculling {
                        @let key = format!("sculling:{}", id);
                        @let rower = find_rower(snapshot, *id);
                        @let rname = rower.map(|r| r.name.as_str()).unwrap_or("");
                        span data-key=(key)
                             data-boat="sculling"
                             data-seat="-1"
                             data-rower=(id)
                             data-name=(rname)
                             class="inline-block px-3 py-1.5 rounded border border-slate-200 cursor-pointer transition rower-content"
                             ":class"={"selected === '" (key) "' ? 'bg-blue-100 ring-2 ring-blue-400 border-blue-400' : 'hover:bg-slate-50'"}
                             "@click"={"select('" (key) "')"} {
                            @if let Some(r) = rower {
                                div class="font-medium text-slate-800 text-sm" { (r.name) }
                                (rower_stats_line(r, flags.show_attributes))
                            } @else {
                                "#" (id)
                            }
                        }
                    }
                }
            }
            div {
                strong class="text-slate-700" { "Benched " }
                span class="text-xs text-slate-500" { "(click to swap into a seat)" }
            }
            div #bench-pills class="flex flex-wrap gap-2 mt-1" {
                @for id in &unplaced.benched {
                    @let key = format!("bench:{}", id);
                    @let rower = find_rower(snapshot, *id);
                    @let rname = rower.map(|r| r.name.as_str()).unwrap_or("");
                    span data-key=(key)
                         data-boat="bench"
                         data-seat="-1"
                         data-rower=(id)
                         data-name=(rname)
                         class="inline-block px-3 py-1.5 rounded border border-slate-200 cursor-pointer transition rower-content"
                         ":class"={"selected === '" (key) "' ? 'bg-blue-100 ring-2 ring-blue-400 border-blue-400' : 'hover:bg-slate-50'"}
                         "@click"={"select('" (key) "')"} {
                        @if let Some(r) = rower {
                            div class="font-medium text-slate-800 text-sm" { (r.name) }
                            (rower_stats_line(r, flags.show_attributes))
                        } @else {
                            "#" (id)
                        }
                    }
                }
            }
        }
    }
}

fn alternatives_panel(
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

/// Colored right-edge bar indicating side preference. Port = red,
/// starboard = green, Either = empty. Notches (gray lines from the
/// bottom) convey strength: more notches = weaker preference.
/// HARD/5 = solid, 4 = 1 notch, 3 = 2, 2 = 3, 1 = 4.
pub(crate) fn side_indicator(rower: Option<&Rower>) -> Markup {
    use lineup_db::rower::types::Side;
    let Some(r) = rower else {
        return html! {};
    };
    let color = match r.side {
        Side::Port => "bg-red-400",
        Side::Starboard => "bg-green-500",
        Side::Either => return html! {},
    };
    let notches = if r.side_strength.is_hard() {
        0
    } else {
        (5 - r.side_strength.as_int()).max(0)
    };
    if notches == 0 {
        return html! {
            td class={"w-2 p-0 " (color)} { "\u{00a0}" }
        };
    }
    // Notches centered vertically: use a repeating gradient sized to
    // the notch block height, positioned at center.
    let notch_h = 2; // px per gray line
    let gap = 3;     // px between lines
    let block_h = notches * (notch_h + gap) - gap; // total notch block height
    let mut stops = Vec::new();
    for i in 0..notches {
        let start = i * (notch_h + gap);
        let end = start + notch_h;
        stops.push(format!("#cbd5e1 {start}px,#cbd5e1 {end}px,transparent {end}px"));
    }
    let gradient = format!(
        "background-image:linear-gradient(to bottom,{});background-size:100% {block_h}px;background-repeat:no-repeat;background-position:center",
        stops.join(",")
    );
    html! {
        td class={"w-2 p-0 " (color)} style=(gradient) { "\u{00a0}" }
    }
}

/// Compact stats line for a rower. When `show_attributes` is false,
/// only shows side preference (non-sensitive); otherwise shows the
/// full weight class / skill / strength / side breakdown.
fn rower_stats_line(r: &Rower, show_attributes: bool) -> Markup {
    if show_attributes {
        html! {
            div class="text-xs text-slate-500" {
                (r.weight_class.short()) " · " (r.skill.short()) " · " (r.strength.short()) " · " (compact_side(r))
            }
        }
    } else {
        use lineup_db::rower::types::Side;
        if r.side != Side::Either {
            html! {
                div class="text-xs text-slate-500" { (compact_side(r)) }
            }
        } else {
            html! {}
        }
    }
}

/// Short rig description for boat card headers, e.g. "port-rigged".
///
/// TODO: `stroke_side` reuses `rower::types::Side` which includes
/// `Either` — boats should have a dedicated `BoatRigSide` enum
/// (Port/Starboard only, no Either) that better captures rigging
/// semantics. The SQL CHECK already forbids Either on boats, but
/// the Rust type doesn't enforce it.
fn rig_label(b: &Boat) -> &'static str {
    use lineup_db::rower::types::Side;
    match b.stroke_side {
        Side::Port => "port-rigged",
        Side::Starboard => "starboard-rigged",
        Side::Either => "unrigged", // unreachable per SQL CHECK
    }
}

/// Compact side label with strength number for lineup cards.
/// e.g. "Port(-4)", "Stbd(+2)", "Either"
fn compact_side(r: &Rower) -> String {
    use lineup_db::rower::types::Side;
    match r.side {
        Side::Either => "Either".to_string(),
        Side::Port => {
            let s = r.side_strength.as_int();
            let pos = if s == 0 { -5 } else { -(6 - s).min(5).max(1) };
            format!("Port({pos:+})")
        }
        Side::Starboard => {
            let s = r.side_strength.as_int();
            let pos = if s == 0 { 5 } else { (6 - s).min(5).max(1) };
            format!("Starboard({pos:+})")
        }
    }
}

/// Whether the cox (seat 0) should be displayed first for this boat.
/// True when the tenant forces stern display or the boat is stern-loaded.
fn cox_first(snapshot: &DbSnapshot, boat_id: BoatId, force_cox_stern: bool) -> bool {
    if force_cox_stern {
        return true;
    }
    snapshot
        .sweep_boats
        .iter()
        .find(|b| b.id == boat_id)
        .map(|b| b.cox_position.cox_first())
        .unwrap_or(true)
}

/// Sort seats for display: stern → bow. When `cox_first` is true,
/// cox (seat 0) comes before all numbered seats; otherwise it comes
/// after them.
fn sort_seats_for_display(seats: &mut Vec<(i32, RowerId)>, cox_at_top: bool) {
    seats.sort_by_key(|(s, _)| {
        if *s == 0 {
            if cox_at_top { i32::MIN } else { i32::MAX }
        } else {
            // Numbered seats display high→low (stern→bow): s8, s7, ..., s1
            -*s
        }
    });
}

/// Human-readable seat label: "cox", "bow" (seat 1), "str" (stroke
/// seat = seat_count), or "s{n}" for everything in between.
pub(crate) fn seat_label(seat: i32, seat_count: i32) -> String {
    if seat == 0 {
        "cox".to_string()
    } else if seat == 1 {
        "bow".to_string()
    } else if seat == seat_count && seat_count > 1 {
        "str".to_string()
    } else {
        format!("s{seat}")
    }
}

/// Colored circle badge for a seat label. Port = red, starboard = green,
/// cox = indigo (neutral). The label text is centered over the circle.
pub(crate) fn seat_badge(boat: Option<&Boat>, seat: i32, label: &str) -> Markup {
    let (bg, text_color) = if seat == 0 {
        ("bg-indigo-100", "text-indigo-700")
    } else if let Some(b) = boat {
        use lineup_db::rower::types::Side;
        match b.seat_side(seat) {
            Some(Side::Port) => ("bg-red-100", "text-red-700"),
            Some(Side::Starboard) => ("bg-green-100", "text-green-700"),
            _ => ("bg-slate-100", "text-slate-500"),
        }
    } else {
        ("bg-slate-100", "text-slate-500")
    };
    html! {
        span class={"inline-flex items-center justify-center w-8 h-8 rounded-full font-mono text-xs font-semibold " (bg) " " (text_color)} {
            (label)
        }
    }
}

fn find_rower(snapshot: &DbSnapshot, id: RowerId) -> Option<&Rower> {
    snapshot.rowers.iter().find(|r| r.id == id)
}
