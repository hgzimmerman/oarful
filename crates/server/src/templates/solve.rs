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

/// A boat in the unified lineup editor, with optional seat assignments.
pub(crate) struct EditorBoat<'a> {
    pub(crate) boat: &'a Boat,
    /// Seat assignments: (seat_position, Option<rower_id>). All seats
    /// are present; None = empty.
    pub(crate) seats: Vec<(i32, Option<RowerId>)>,
    /// Whether this boat is active (shown with cards). Inactive boats
    /// are toggled off in the boat selector.
    pub(crate) active: bool,
}

/// Data for the unified lineup editor.
pub(crate) struct EditorData<'a> {
    pub(crate) boats: Vec<EditorBoat<'a>>,
    pub(crate) pool: Vec<&'a Rower>,
    pub(crate) sculling: Vec<&'a Rower>,
}

impl<'a> EditorData<'a> {
    /// Build from a snapshot with no solver result — all boats empty,
    /// all available rowers in pool.
    pub(crate) fn empty(snapshot: &'a DbSnapshot) -> Self {
        let boats = snapshot.sweep_boats.iter().map(|b| {
            let has_cox = b.has_cox.as_bool();
            let mut seats = Vec::new();
            if has_cox { seats.push((0, None)); }
            for s in 1..=b.seat_count { seats.push((s, None)); }
            EditorBoat { boat: b, seats, active: true }
        }).collect();
        let pool: Vec<&Rower> = snapshot.available_rowers().collect();
        Self { boats, pool, sculling: vec![] }
    }

    /// Build from a solver result — used boats have seats filled,
    /// unused boats are inactive, unplaced rowers in pool/sculling.
    pub(crate) fn from_solve(
        snapshot: &'a DbSnapshot,
        primary: &lineup_solver::ProposedSolution,
    ) -> Self {
        let filled_map: HashMap<BoatId, HashMap<i32, RowerId>> = primary
            .lineups
            .iter()
            .map(|l| {
                let seat_map: HashMap<i32, RowerId> = l.seats.iter().copied().collect();
                (l.boat_id, seat_map)
            })
            .collect();

        let boats = snapshot.sweep_boats.iter().map(|b| {
            let lineup = primary.lineups.iter().find(|l| l.boat_id == b.id);
            let active = lineup.map(|l| l.used).unwrap_or(false);
            let seat_map = filled_map.get(&b.id);
            let has_cox = b.has_cox.as_bool();
            let mut seats = Vec::new();
            if has_cox {
                seats.push((0, seat_map.and_then(|m| m.get(&0).copied())));
            }
            for s in 1..=b.seat_count {
                seats.push((s, seat_map.and_then(|m| m.get(&s).copied())));
            }
            EditorBoat { boat: b, seats, active }
        }).collect();

        let pool: Vec<&Rower> = primary.unplaced.benched.iter()
            .filter_map(|id| snapshot.rowers.iter().find(|r| r.id == *id))
            .collect();
        let sculling: Vec<&Rower> = primary.unplaced.to_sculling.iter()
            .filter_map(|id| snapshot.rowers.iter().find(|r| r.id == *id))
            .collect();

        Self { boats, pool, sculling }
    }
}

/// Unified lineup editor — renders boat selector, boat cards with
/// seats (filled or empty), and the rower pool. Used for both the
/// pre-generate landing and the post-generate result.
pub(crate) fn lineup_editor(
    snapshot: &DbSnapshot,
    date: NaiveDate,
    editor: &EditorData,
    flags: &DisplayFlags,
    knobs: &SolveKnobs,
) -> Markup {
    let commit_action = format!("/commit-lineup/{date}");

    html! {
        section class="bg-white rounded-lg shadow p-4 sm:p-6"
               x-data="lineupEditor()" {

            div class="flex items-center justify-between mb-4 flex-wrap gap-2" {
                h2 class="text-xl font-bold text-slate-800" { "Lineup" }
                div class="flex items-center gap-2" {
                    template x-if="selected" {
                        span class="text-xs text-blue-600" {
                            "Click another to swap"
                            " · "
                            button type="button"
                                   class="underline text-amber-600 hover:text-amber-800"
                                   "@click"="toBench(selected)" {
                                "bench"
                            }
                            " · or click again to cancel"
                        }
                    }
                    form method="post" action=(commit_action) x-ref="commitForm" {
                        div x-ref="seatInputs" {}
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition" {
                            "Commit lineup"
                        }
                    }
                }
            }

            // Boat selector pills
            div class="flex flex-wrap gap-2 mb-4" {
                @for eb in &editor.boats {
                    @let bid = eb.boat.id.as_int();
                    @let active_class = if eb.active {
                        "px-3 py-1 rounded-full text-sm font-medium bg-slate-800 text-white cursor-pointer"
                    } else {
                        "px-3 py-1 rounded-full text-sm font-medium bg-slate-200 text-slate-500 cursor-pointer"
                    };
                    button type="button" class=(active_class)
                           data-boat-id=(bid)
                           "@click"={"toggleBoat(" (bid) ")"} {
                        (eb.boat.name)
                        " ("
                        (eb.boat.seat_count)
                        @if eb.boat.has_cox.as_bool() { "+" }
                        ")"
                    }
                }
            }

            // Boat cards
            div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4" {
                @for eb in &editor.boats {
                    @if eb.active {
                        (editor_boat_card(snapshot, eb, flags))
                    }
                }
            }

            // Rower pool
            div class="pt-4 border-t border-slate-200 text-sm space-y-2" {
                @if !editor.sculling.is_empty() {
                    div {
                        strong class="text-slate-700" { "To sculling " }
                    }
                    div class="flex flex-wrap gap-2 mt-1" {
                        @for r in &editor.sculling {
                            @let key = format!("sculling:{}", r.id);
                            @let stats = format!(
                                "{} · {} · {} · {}",
                                r.weight_class.short(), r.skill.short(), r.strength.short(), compact_side(r)
                            );
                            span data-key=(key)
                                 data-boat="sculling"
                                 data-seat="-1"
                                 data-rower=(r.id)
                                 data-name=(r.name)
                                 data-stats=(stats)
                                 class="inline-block px-3 py-1.5 rounded border border-slate-200 cursor-pointer transition rower-content hover:bg-slate-50"
                                 ":class"={"selected === '" (key) "' ? 'bg-blue-100 ring-2 ring-blue-400 border-blue-400' : 'hover:bg-slate-50'"}
                                 "@click"={"select('" (key) "')"} {
                                div class="font-medium text-slate-800 text-sm" { (r.name) }
                                div class="text-xs text-slate-500" { (stats) }
                            }
                        }
                    }
                }

                div {
                    strong class="text-slate-700" { "Available " }
                    span class="text-xs text-slate-500" { "(click to place in a seat)" }
                }
                div #bench-pills class="flex flex-wrap gap-2 mt-1" {
                    @for r in &editor.pool {
                        @let key = format!("bench:{}", r.id);
                        @let stats = format!(
                            "{} · {} · {} · {}",
                            r.weight_class.short(), r.skill.short(), r.strength.short(), compact_side(r)
                        );
                        span data-key=(key)
                             data-boat="bench"
                             data-seat="-1"
                             data-rower=(r.id)
                             data-name=(r.name)
                             data-stats=(stats)
                             class="inline-block px-3 py-1.5 rounded border border-slate-200 cursor-pointer transition rower-content hover:bg-slate-50"
                             ":class"={"selected === '" (key) "' ? 'bg-blue-100 ring-2 ring-blue-400 border-blue-400' : 'hover:bg-slate-50'"}
                             "@click"={"select('" (key) "')"} {
                            div class="font-medium text-slate-800 text-sm" { (r.name) }
                            div class="text-xs text-slate-500" { (stats) }
                        }
                    }
                }
            }
        }

        // Unified Alpine editor logic
        script {
            (maud::PreEscaped(editor_js(knobs)))
        }
    }
}

/// Render one boat card in the unified editor.
fn editor_boat_card(snapshot: &DbSnapshot, eb: &EditorBoat, flags: &DisplayFlags) -> Markup {
    let boat = eb.boat;
    let seat_count = boat.seat_count;
    let cox_at_top = cox_first(snapshot, boat.id, flags.force_cox_stern);
    let mut seats = eb.seats.clone();
    seats.sort_by_key(|(s, _)| {
        if *s == 0 {
            if cox_at_top { i32::MIN } else { i32::MAX }
        } else {
            -*s
        }
    });

    html! {
        div class="border border-slate-200 rounded-lg overflow-hidden"
             data-editor-boat=(boat.id)
             data-hidden="false" {
            div class="bg-slate-100 px-4 py-2 border-b border-slate-200" {
                strong class="text-slate-800" { (boat.name) }
                span class="text-xs text-slate-500 ml-2" {
                    "(" (seat_count) "+"
                    ", " (rig_label(boat))
                    ")"
                }
            }
            table class="w-full text-sm" {
                tbody {
                    @for (seat, maybe_rower) in &seats {
                        @let key = format!("{}:{}", boat.id, seat);
                        @let label = seat_label(*seat, seat_count);
                        @let rower = maybe_rower.and_then(|id| find_rower(snapshot, id));
                        @let rower_id_str = maybe_rower.map(|id| id.as_int().to_string()).unwrap_or_default();
                        @let rower_name = rower.map(|r| r.name.as_str()).unwrap_or("");
                        @let stats_text = rower.map(|r| format!(
                            "{} · {} · {} · {}",
                            r.weight_class.short(), r.skill.short(), r.strength.short(), compact_side(r)
                        )).unwrap_or_default();
                        @let is_designated_cox = rower.map(|r| r.is_designated_cox.as_bool()).unwrap_or(false);
                        @let is_locked = maybe_rower.map(|rid| flags.locked_seats.contains(&(rid, boat.id, *seat))).unwrap_or(false);
                        @let is_empty = maybe_rower.is_none();
                        @let row_base = if is_empty {
                            "border-b border-slate-100 last:border-0 cursor-pointer transition bg-slate-50"
                        } else if is_locked {
                            "border-b border-slate-100 last:border-0 cursor-pointer transition bg-violet-50 border-l-4 border-l-violet-400"
                        } else if is_designated_cox {
                            "border-b border-slate-100 last:border-0 cursor-pointer transition border-l-4 border-l-indigo-400"
                        } else {
                            "border-b border-slate-100 last:border-0 cursor-pointer transition"
                        };
                        @let lock_val = format!("{}:{}:{}", rower_id_str, boat.id, seat);
                        tr data-key=(key)
                           data-boat=(boat.id)
                           data-seat=(seat)
                           data-rower=(rower_id_str)
                           data-name=(rower_name)
                           data-stats=(stats_text)
                           class=(row_base)
                           ":class"={"selected === '" (key) "' ? 'bg-blue-100 ring-2 ring-inset ring-blue-400' : 'hover:bg-slate-50'"}
                           "@click"={"select('" (key) "')"} {
                            td class="px-4 py-2 w-12" {
                                (seat_badge(Some(boat), *seat, &label))
                            }
                            td class="px-4 py-2 rower-content" {
                                @if let Some(r) = rower {
                                    div class="font-medium text-slate-800" { (r.name) }
                                    (rower_stats_line(r, flags.show_attributes))
                                } @else if is_empty {
                                    span class="text-slate-400 italic" { "\u{2014} empty \u{2014}" }
                                } @else {
                                    span class="text-slate-400 italic" { "unknown" }
                                }
                            }
                            @if !is_empty {
                                td class="w-8 text-center" {
                                    button type="button"
                                           class="text-xs hover:text-violet-700"
                                           title=(if is_locked { "Unlock seat" } else { "Lock seat" })
                                           data-lock=(lock_val)
                                           "@click.stop"="toggleLock($event.currentTarget.dataset.lock)" {
                                        @if is_locked { "🔒" } @else { "🔓" }
                                    }
                                }
                            } @else {
                                td class="w-8" {}
                            }
                            (side_indicator(rower))
                        }
                    }
                }
            }
        }
    }
}

/// Alpine JS for the unified lineup editor.
fn editor_js(knobs: &SolveKnobs) -> String {
    format!(r#"
function lineupEditor() {{
    return {{
        selected: null,
        init() {{ this.rebuildInputs(); }},
        select(key) {{
            if (!this.selected) {{
                this.selected = key;
            }} else if (this.selected === key) {{
                this.selected = null;
            }} else {{
                this.doSwap(this.selected, key);
                this.selected = null;
            }}
        }},
        doSwap(a, b) {{
            var elA = this.$root.querySelector('[data-key="' + a + '"]');
            var elB = this.$root.querySelector('[data-key="' + b + '"]');
            if (!elA || !elB) return;
            var tmpRower = elA.dataset.rower;
            var tmpName = elA.dataset.name;
            var tmpStats = elA.dataset.stats || '';
            elA.dataset.rower = elB.dataset.rower;
            elA.dataset.name = elB.dataset.name;
            elA.dataset.stats = elB.dataset.stats || '';
            elB.dataset.rower = tmpRower;
            elB.dataset.name = tmpName;
            elB.dataset.stats = tmpStats;
            this.renderSlot(elA);
            this.renderSlot(elB);
            this.rebuildInputs();
        }},
        toBench(key) {{
            var el = this.$root.querySelector('[data-key="' + key + '"]');
            if (!el || el.dataset.boat === 'bench' || el.dataset.boat === 'sculling') return;
            var rowerId = el.dataset.rower;
            var rowerName = el.dataset.name;
            var rowerStats = el.dataset.stats || '';
            if (!rowerId) return;
            el.dataset.rower = '';
            el.dataset.name = '';
            el.dataset.stats = '';
            this.renderSlot(el);
            this.addPoolPill(rowerId, rowerName, rowerStats);
            this.selected = null;
            this.rebuildInputs();
        }},
        addPoolPill(rowerId, rowerName, rowerStats) {{
            var container = this.$root.querySelector('#bench-pills');
            if (!container) return;
            var key = 'bench:' + rowerId;
            var span = document.createElement('span');
            span.dataset.key = key;
            span.dataset.boat = 'bench';
            span.dataset.seat = '-1';
            span.dataset.rower = rowerId;
            span.dataset.name = rowerName || '';
            span.dataset.stats = rowerStats || '';
            span.className = 'inline-block px-3 py-1.5 rounded border border-slate-200 cursor-pointer transition rower-content hover:bg-slate-50';
            span.setAttribute('@click', "select('" + key + "')");
            span.setAttribute(':class', "selected === '" + key + "' ? 'bg-blue-100 ring-2 ring-blue-400 border-blue-400' : 'hover:bg-slate-50'");
            var html = '<div class="font-medium text-slate-800 text-sm">' + this.esc(rowerName || '#' + rowerId) + '</div>';
            if (rowerStats) html += '<div class="text-xs text-slate-500">' + this.esc(rowerStats) + '</div>';
            span.innerHTML = html;
            container.appendChild(span);
        }},
        renderSlot(el) {{
            var rowerId = el.dataset.rower;
            var rowerName = el.dataset.name;
            var stats = el.dataset.stats || '';
            var content = el.querySelector('.rower-content');
            if (!content) content = el.tagName === 'SPAN' ? el : null;
            if (!content) return;
            if (!rowerId) {{
                content.innerHTML = '<span class="text-slate-400 italic">\u2014 empty \u2014</span>';
            }} else {{
                var html = '<div class="font-medium text-slate-800' +
                    (el.tagName === 'SPAN' ? ' text-sm' : '') + '">' +
                    this.esc(rowerName || '#' + rowerId) + '</div>';
                if (stats) html += '<div class="text-xs text-slate-500">' + this.esc(stats) + '</div>';
                content.innerHTML = html;
            }}
        }},
        toggleBoat(boatId) {{
            var card = this.$root.querySelector('[data-editor-boat="' + boatId + '"]');
            var pill = this.$root.querySelector('[data-boat-id="' + boatId + '"]');
            if (!card) return;
            var isHidden = card.dataset.hidden === 'true';
            if (isHidden) {{
                // Show the boat.
                card.style.display = '';
                card.dataset.hidden = 'false';
                if (pill) pill.className = 'px-3 py-1 rounded-full text-sm font-medium bg-slate-800 text-white cursor-pointer';
            }} else {{
                // Bench all rowers from this boat, then hide.
                var rows = Array.from(card.querySelectorAll('tr[data-rower]'));
                var self = this;
                rows.forEach(function(row) {{
                    var rid = row.dataset.rower;
                    if (rid) {{
                        var rn = row.dataset.name;
                        var rs = row.dataset.stats || '';
                        row.dataset.rower = '';
                        row.dataset.name = '';
                        row.dataset.stats = '';
                        self.renderSlot(row);
                        self.addPoolPill(rid, rn, rs);
                    }}
                }});
                card.style.display = 'none';
                card.dataset.hidden = 'true';
                if (pill) pill.className = 'px-3 py-1 rounded-full text-sm font-medium bg-slate-200 text-slate-500 cursor-pointer';
            }}
            this.rebuildInputs();
        }},
        esc(s) {{
            var d = document.createElement('div');
            d.textContent = s;
            return d.innerHTML;
        }},
        toggleLock(lockVal) {{
            var form = document.querySelector('form[hx-get]');
            if (!form) return;
            var existing = form.querySelector('input[name="lock"][value="' + lockVal + '"]');
            var btn = this.$root.querySelector('[data-lock="' + lockVal + '"]');
            var row = btn ? btn.closest('tr') : null;
            if (existing) {{
                existing.remove();
                if (btn) btn.textContent = '\uD83D\uDD13';
                if (row) row.classList.remove('bg-violet-50', 'border-l-4', 'border-l-violet-400');
            }} else {{
                var inp = document.createElement('input');
                inp.type = 'hidden';
                inp.name = 'lock';
                inp.value = lockVal;
                form.appendChild(inp);
                if (btn) btn.textContent = '\uD83D\uDD12';
                if (row) row.classList.add('bg-violet-50', 'border-l-4', 'border-l-violet-400');
            }}
        }},
        rebuildInputs() {{
            var container = this.$refs.seatInputs;
            if (!container) return;
            container.innerHTML = '';
            var placements = [];
            this.$root.querySelectorAll('tr[data-boat][data-seat][data-rower]').forEach(function(el) {{
                if (el.dataset.boat === 'bench' || el.dataset.boat === 'sculling') return;
                if (!el.dataset.rower) return;
                var val = el.dataset.boat + ':' + el.dataset.seat + ':' + el.dataset.rower;
                var inp = document.createElement('input');
                inp.type = 'hidden';
                inp.name = 'seat';
                inp.value = val;
                container.appendChild(inp);
                placements.push(el.dataset.rower + ':' + el.dataset.boat + ':' + el.dataset.seat);
            }});
            // Inject placements as locks into the knobs form for Generate.
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
    flags: &DisplayFlags,
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

    let editor = EditorData::empty(snapshot);

    html! {
        (page_header(&format!("Set Lineups · {date}"), Some(&subtitle)))
        div class="px-4 sm:px-8 py-6 space-y-6 max-w-6xl mx-auto" {
            (knobs_form(date, knobs, committed_practices, has_committed, custom_profiles))
            (walkon_section(date, &unavailable, knobs))
            (lineup_editor(snapshot, date, &editor, flags, knobs))
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

    let editor = if result.status == SolveStatus::Satisfied {
        EditorData::from_solve(snapshot, &result.primary)
    } else {
        EditorData::empty(snapshot)
    };

    html! {
        (page_header(&format!("Set Lineups · {date}"), Some(&subtitle)))
        div class="px-4 sm:px-8 py-6 space-y-6 max-w-6xl mx-auto" {
            (knobs_form(date, knobs, committed_practices, true, custom_profiles))
            (status_banner(date, &result.status, &result.diagnostics))
            (lineup_editor(snapshot, date, &editor, flags, knobs))

            @if result.status == SolveStatus::Satisfied && !result.alternatives.is_empty() {
                (alternatives_panel(snapshot, &result.primary, &result.alternatives, flags))
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

                // Solver knobs
                div class="grid grid-cols-2 md:grid-cols-4 gap-4 items-end" {
                    // Partial fill — segmented (Off / 1 / 2)
                    div {
                        label class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                            "Partial fill"
                        }
                        div class="inline-flex rounded border border-slate-300 overflow-hidden text-sm" {
                            @for (val, lbl) in &[(0, "Off"), (1, "1 empty"), (2, "2 empty")] {
                                @let active = knobs.partial == *val;
                                @let cls = if active {
                                    "px-3 py-1.5 font-semibold bg-slate-800 text-white"
                                } else {
                                    "px-3 py-1.5 text-slate-700 hover:bg-slate-100"
                                };
                                button type="submit" name="partial" value=(val) class=(cls) {
                                    (lbl)
                                }
                            }
                        }
                        // Hidden fallback when Generate (not a partial button) is clicked
                        input type="hidden" name="partial" value=(knobs.partial);
                        p class="text-xs text-slate-500 mt-1" { "Empty optional seats per boat" }
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
                                    "px-3 py-1.5 font-semibold bg-slate-800 text-white"
                                } else {
                                    "px-3 py-1.5 text-slate-700 hover:bg-slate-100"
                                };
                                button type="submit" name="alts" value=(n) class=(cls) {
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
                                  oninput="document.getElementById('budget-val').textContent = this.value + 's'";
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
                                  oninput="document.getElementById('novelty-val').textContent = this.value === '0' ? 'Off' : this.value";
                            span #novelty-val class="text-sm font-mono text-slate-700 w-8" {
                                @if knobs.novelty == 0 { "Off" } @else { (knobs.novelty) }
                            }
                        }
                        p class="text-xs text-slate-500 mt-1" { "Avoid repeating recent lineups" }
                    }
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

// primary_panel, swap_boat_card, swap_unplaced_block removed —
// replaced by the unified lineup_editor.

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
