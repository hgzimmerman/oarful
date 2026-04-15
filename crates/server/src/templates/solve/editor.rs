//! Unified lineup editor — boat selector, boat cards with seats
//! (filled or empty), and the rower pool.

use std::collections::{HashMap, HashSet};

use lineup_db::{
    boat::types::BoatId,
    practice::PracticeId,
    rower::{types::RowerId, Rower},
    snapshot::DbSnapshot,
};
use maud::{html, Markup};

use super::{
    compact_side, cox_first, find_rower, rig_label, rower_stats_line, seat_badge, seat_label,
    side_indicator,
};

/// Display flags threaded through lineup card rendering.
#[derive(Clone)]
pub(crate) struct DisplayFlags {
    pub(crate) show_attributes: bool,
    pub(crate) force_cox_stern: bool,
    /// Explicit coach locks — always honored by solver.
    pub(crate) locked_seats: HashSet<(RowerId, BoatId, i32)>,
    /// Dirty pins — manual edits since last generate.
    pub(crate) pinned_seats: HashSet<(RowerId, BoatId, i32)>,
    /// Was-pinned — honored last generate, not constrained now.
    pub(crate) was_pinned_seats: HashSet<(RowerId, BoatId, i32)>,
    /// Boat-level pin states.
    pub(crate) pinned_boats: HashSet<BoatId>,
    pub(crate) was_pinned_boats: HashSet<BoatId>,
    pub(crate) locked_boats: HashSet<BoatId>,
    /// Boats currently in use by another team's committed lineup.
    /// Maps boat_id → team name for display.
    pub(crate) boats_in_use_by: HashMap<BoatId, String>,
}

/// A boat in the unified lineup editor, with optional seat assignments.
struct EditorBoat<'a> {
    boat: &'a lineup_db::boat::Boat,
    /// Seat assignments: (seat_position, Option<rower_id>). All seats
    /// are present; None = empty.
    seats: Vec<(i32, Option<RowerId>)>,
    /// Whether this boat is active (shown with cards). Inactive boats
    /// are toggled off in the boat selector.
    active: bool,
}

/// Data for the unified lineup editor.
pub(crate) struct EditorData<'a> {
    boats: Vec<EditorBoat<'a>>,
    pool: Vec<&'a Rower>,
    sculling: Vec<&'a Rower>,
}

impl<'a> EditorData<'a> {
    /// Sort key for pool rowers: port-biased first (negative), then
    /// either (zero), then starboard-biased (positive). Within a side,
    /// stronger preference comes first.
    fn side_sort_key(r: &Rower) -> i32 {
        use lineup_db::rower::types::Side;
        let strength = r.side_strength.as_int(); // 0 = hard lock, 5 = flexible
        let bias = 5 - strength; // higher = stronger preference
        match r.side {
            Side::Port => -(bias + 1),      // -6..-1
            Side::Either => 0,
            Side::Starboard => bias + 1,    // 1..6
        }
    }

    fn sort_pool(pool: &mut [&Rower]) {
        pool.sort_by_key(|r| Self::side_sort_key(r));
    }

    /// Build from a snapshot with no solver result — all boats empty,
    /// all available rowers in pool. When `default_boats` is non-empty,
    /// only those boats start active; otherwise all boats are active.
    pub(crate) fn empty(snapshot: &'a DbSnapshot, default_boats: &HashSet<BoatId>) -> Self {
        let use_defaults = !default_boats.is_empty();
        let boats = snapshot.boats.iter().map(|b| {
            let has_cox = b.has_cox.as_bool();
            let mut seats = Vec::new();
            if has_cox { seats.push((0, None)); }
            for s in 1..=b.seat_count { seats.push((s, None)); }
            let active = if use_defaults { default_boats.contains(&b.id) } else { true };
            EditorBoat { boat: b, seats, active }
        }).collect();
        let mut pool: Vec<&Rower> = snapshot.available_rowers().collect();
        Self::sort_pool(&mut pool);
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

        let boats = snapshot.boats.iter().map(|b| {
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

        let mut pool: Vec<&Rower> = primary.unplaced.benched.iter()
            .filter_map(|id| snapshot.rowers.iter().find(|r| r.id == *id))
            .collect();
        Self::sort_pool(&mut pool);

        Self { boats, pool, sculling: vec![] }
    }

    /// Build from explicit placements — used by the editor endpoint
    /// after client-side operations (swap, bench, toggle boat). The
    /// server re-renders the editor with correct indicators/stats.
    pub(crate) fn from_placements(
        snapshot: &'a DbSnapshot,
        placements: &HashMap<BoatId, HashMap<i32, RowerId>>,
        active_boats: &HashSet<BoatId>,
    ) -> Self {
        let boats = snapshot.boats.iter().map(|b| {
            let active = active_boats.contains(&b.id);
            let seat_map = placements.get(&b.id);
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

        // Pool = available rowers not placed in any seat.
        let placed: HashSet<RowerId> = placements
            .values()
            .flat_map(|m| m.values().copied())
            .collect();
        let mut pool: Vec<&Rower> = snapshot.available_rowers()
            .filter(|r| !placed.contains(&r.id))
            .collect();
        Self::sort_pool(&mut pool);

        Self { boats, pool, sculling: vec![] }
    }
}

/// Unified lineup editor — renders boat selector, boat cards with
/// seats (filled or empty), and the rower pool. Used for both the
/// pre-generate landing and the post-generate result.
/// A rower from another team available during this practice window.
pub(crate) struct OtherTeamRower {
    pub(crate) rower: Rower,
    pub(crate) team_name: String,
}

pub(crate) fn lineup_editor(
    snapshot: &DbSnapshot,
    practice_id: PracticeId,
    editor: &EditorData,
    flags: &DisplayFlags,
    unavailable: &[&Rower],
    walkon_ids: &[String],
    other_team_rowers: &[OtherTeamRower],
) -> Markup {
    let commit_action = format!("/commit-lineup/{practice_id}");
    let editor_url = format!("/solve/{practice_id}/editor");

    html! {
        section #lineup-editor class="bg-white rounded-lg shadow p-4 sm:p-6"
               data-practice-id=(practice_id)
               data-editor-url=(editor_url)
               x-data="lineupEditor()" {

            div class="flex items-center justify-between mb-1" {
                h2 class="text-xl font-bold text-slate-800" { "Lineup" }
                div class="no-print" {
                    form method="post" action=(commit_action) {
                        // Server-rendered hidden inputs for commit.
                        @for eb in &editor.boats {
                            @if eb.active {
                                @for (seat, maybe_rower) in &eb.seats {
                                    @if let Some(rower_id) = maybe_rower {
                                        input type="hidden" name="seat"
                                              value={(eb.boat.id) ":" (seat) ":" (rower_id)};
                                    }
                                }
                            }
                        }
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition" {
                            "Commit lineup"
                        }
                    }
                }
            }
            // Selection hint — fixed height so it doesn't cause reflow.
            div class="h-6 mb-2 no-print" {
                span class="text-xs text-blue-600"
                     x-show="selected && !selectedBoat"
                     x-cloak {
                    "Click another to swap · or click again to cancel"
                }
                span class="text-xs text-blue-600"
                     x-show="selectedBoat"
                     x-cloak {
                    "Click a boat pill to transfer rowers · or click Transfer again to cancel"
                }
            }

            // Boat selector pills
            div class="flex flex-wrap items-center gap-2 mb-4 no-print" {
                @for eb in &editor.boats {
                    @let bid = eb.boat.id.as_int();
                    @let active_class = if eb.active {
                        "px-4 py-2 rounded-full text-sm font-medium bg-slate-800 text-white cursor-pointer"
                    } else {
                        "px-4 py-2 rounded-full text-sm font-medium bg-slate-200 text-slate-500 cursor-pointer"
                    };
                    @let pill_target_class = "px-4 py-2 rounded-full text-sm font-medium bg-blue-600 text-white cursor-pointer ring-2 ring-blue-400";
                    @let in_use_team = flags.boats_in_use_by.get(&eb.boat.id);
                    button type="button"
                           class=(active_class)
                           data-boat-id=(bid)
                           ":class"={"selectedBoat !== null && selectedBoat !== " (bid) " ? '" (pill_target_class) "' : '" (active_class) "'"}
                           "@click"={"boatPillClick(" (bid) ")"} {
                        (eb.boat.name)
                        " ("
                        (eb.boat.seat_count)
                        @if eb.boat.has_cox.as_bool() { "+" }
                        ")"
                        @if let Some(_team) = in_use_team {
                            span class="ml-1 inline-block px-1.5 py-0.5 text-[10px] font-semibold bg-amber-200 text-amber-800 rounded-full leading-none" {
                                "In use"
                            }
                        }
                    }
                }
                @if editor.boats.len() > 2 {
                    span class="inline-flex items-center gap-1 ml-1" {
                        span class="text-xs text-slate-400" { "|" }
                        button type="button"
                               class="px-2 py-1 text-xs text-slate-500 hover:text-slate-800 hover:bg-slate-100 font-medium rounded"
                               "@click"="selectAllBoats()" { "All" }
                        button type="button"
                               class="px-2 py-1 text-xs text-slate-500 hover:text-slate-800 hover:bg-slate-100 font-medium rounded"
                               "@click"="deselectAllBoats()" { "None" }
                    }
                }
            }

            // Boat cards (all rendered; inactive ones hidden via style)
            div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4" {
                @for eb in &editor.boats {
                    (editor_boat_card(snapshot, eb, flags))
                }
            }

            // Walk-on + Rower pool
            div class="pt-4 border-t border-slate-200 text-sm space-y-2" {
                // Walk-on: add unavailable rowers to the pool.
                @let has_addable = unavailable.iter().any(|r| !walkon_ids.contains(&r.id.as_int().to_string()));
                @if has_addable || !walkon_ids.is_empty() {
                    div class="flex items-center gap-2 flex-wrap mb-2" {
                        @if !walkon_ids.is_empty() {
                            span class="text-xs font-semibold text-slate-700 uppercase tracking-wide" { "Walk-ons:" }
                            @for id_str in walkon_ids {
                                @if let Ok(id) = id_str.parse::<lineup_db::rower::types::RowerId>() {
                                    @let name = snapshot.rowers.iter().find(|r| r.id == id).map(|r| r.name.as_str()).unwrap_or("?");
                                    span class="inline-block px-2 py-0.5 text-xs bg-emerald-100 text-emerald-800 rounded-full" {
                                        (name)
                                    }
                                }
                            }
                        }
                        @if has_addable {
                            @let walkon_action = format!("/solve/{practice_id}");
                            form method="get" action=(walkon_action)
                                 hx-get=(walkon_action)
                                 hx-target="#content"
                                 hx-push-url="true"
                                 class="inline-flex items-center gap-2" {
                                // Carry existing knobs through.
                                input type="hidden" name="partial" value="0";
                                @for w in walkon_ids {
                                    input type="hidden" name="walkon" value=(w);
                                }
                                select name="walkon"
                                       class="border border-slate-300 rounded px-2 py-1.5 text-sm focus:border-slate-500 focus:outline-none" {
                                    @for r in unavailable {
                                        @if !walkon_ids.contains(&r.id.as_int().to_string()) {
                                            option value=(r.id) { (r.name) }
                                        }
                                    }
                                }
                                button type="submit"
                                       class="text-xs font-semibold text-emerald-700 hover:text-emerald-900 uppercase tracking-wide" {
                                    "+ Walk-on"
                                }
                            }
                        }
                    }
                }
                @if !editor.sculling.is_empty() {
                    div {
                        strong class="text-slate-700" { "To sculling " }
                    }
                    div class="flex flex-wrap gap-2 mt-1" {
                        @for r in &editor.sculling {
                            @let key = format!("sculling:{}", r.id);
                            @let side_border = if r.is_designated_cox.as_bool() {
                                "border-r-2 border-r-indigo-400"
                            } else {
                                match r.side {
                                    lineup_db::rower::types::Side::Port => "border-r-2 border-r-red-400",
                                    lineup_db::rower::types::Side::Starboard => "border-r-2 border-r-green-500",
                                    lineup_db::rower::types::Side::Either => "",
                                }
                            };
                            span data-key=(key)
                                 data-boat="sculling"
                                 data-seat="-1"
                                 data-rower=(r.id)
                                 class={"inline-block px-3 py-2 rounded border border-slate-200 cursor-pointer transition hover:bg-slate-50 " (side_border)}
                                 ":class"={"selected === '" (key) "' ? 'bg-blue-100 ring-2 ring-blue-400 border-blue-400' : 'hover:bg-slate-50'"}
                                 "@click"={"select('" (key) "')"} {
                                div class="font-medium text-slate-800 text-sm" { (r.name) }
                                div class="text-xs text-slate-500" {
                                    (r.weight_class.short()) " · " (r.skill.short()) " · " (r.strength.short()) " · " (compact_side(r))
                                }
                            }
                        }
                    }
                }

                div {
                    strong class="text-slate-700" { "Available " }
                    span class="text-xs text-slate-500" { "(click to place in a seat)" }
                }
                @let has_boated = editor.boats.iter().any(|eb| eb.active && eb.seats.iter().any(|(_, r)| r.is_some()));
                div class="flex flex-wrap gap-2 mt-1" {
                    // Empty bench slot — drop target for moving a rower out of a boat.
                    @if has_boated {
                        span data-key="bench:empty"
                             data-boat="bench"
                             data-seat="-1"
                             data-rower=""
                             class="inline-block px-3 py-2 rounded border border-dashed border-slate-300 cursor-pointer transition hover:bg-slate-50"
                             ":class"={"selected === 'bench:empty' ? 'bg-blue-100 ring-2 ring-blue-400 border-blue-400' : 'hover:bg-slate-50'"}
                             "@click"="select('bench:empty')" {
                            span class="text-slate-400 italic text-sm" { "\u{2014} bench \u{2014}" }
                        }
                    }
                    @for r in &editor.pool {
                        @let key = format!("bench:{}", r.id);
                        @let side_border = if r.is_designated_cox.as_bool() {
                            "border-r-2 border-r-indigo-400"
                        } else {
                            match r.side {
                                lineup_db::rower::types::Side::Port => "border-r-2 border-r-red-400",
                                lineup_db::rower::types::Side::Starboard => "border-r-2 border-r-green-500",
                                lineup_db::rower::types::Side::Either => "",
                            }
                        };
                        span data-key=(key)
                             data-boat="bench"
                             data-seat="-1"
                             data-rower=(r.id)
                             class={"inline-block px-3 py-2 rounded border border-slate-200 cursor-pointer transition hover:bg-slate-50 " (side_border)}
                             ":class"={"selected === '" (key) "' ? 'bg-blue-100 ring-2 ring-blue-400 border-blue-400' : 'hover:bg-slate-50'"}
                             "@click"={"select('" (key) "')"} {
                            div class="font-medium text-slate-800 text-sm" { (r.name) }
                            div class="text-xs text-slate-500" {
                                (r.weight_class.short()) " · " (r.skill.short()) " · " (r.strength.short()) " · " (compact_side(r))
                            }
                        }
                    }
                }

                // Available from other teams — informational only.
                @if !other_team_rowers.is_empty() {
                    div class="mt-3" {
                        strong class="text-amber-700" { "Available from other teams " }
                        span class="text-xs text-slate-500" { "(use walk-on to pull in)" }
                    }
                    div class="flex flex-wrap gap-2 mt-1" {
                        @for otr in other_team_rowers {
                            @let r = &otr.rower;
                            @let side_border = if r.is_designated_cox.as_bool() {
                                "border-r-2 border-r-indigo-400"
                            } else {
                                match r.side {
                                    lineup_db::rower::types::Side::Port => "border-r-2 border-r-red-400",
                                    lineup_db::rower::types::Side::Starboard => "border-r-2 border-r-green-500",
                                    lineup_db::rower::types::Side::Either => "",
                                }
                            };
                            span class={"inline-block px-3 py-2 rounded border border-amber-200 bg-amber-50 " (side_border)} {
                                div class="font-medium text-amber-900 text-sm" {
                                    (r.name)
                                    span class="ml-1 text-[10px] font-normal text-amber-600" { "(" (&otr.team_name) ")" }
                                }
                                div class="text-xs text-amber-700" {
                                    (r.weight_class.short()) " · " (r.skill.short()) " · " (r.strength.short()) " · " (compact_side(r))
                                }
                            }
                        }
                    }
                }
            }
        }

        // Alpine editor logic — selection + HTMX re-render
        script {
            (maud::PreEscaped(editor_js()))
        }

        // OOB swap: inject active boats + carry through pin state.
        // Does NOT auto-lock placements — only explicit state is preserved.
        div #editor-knob-state hx-swap-oob="innerHTML" {
            @for eb in &editor.boats {
                @if eb.active {
                    input type="hidden" name="boat" value=(eb.boat.id);
                }
            }
            @for &(rid, bid, seat) in &flags.locked_seats {
                input type="hidden" name="lock" value={(rid) ":" (bid) ":" (seat)};
            }
            @for &(rid, bid, seat) in &flags.pinned_seats {
                input type="hidden" name="pin" value={(rid) ":" (bid) ":" (seat)};
            }
            @for &(rid, bid, seat) in &flags.was_pinned_seats {
                input type="hidden" name="was_pin" value={(rid) ":" (bid) ":" (seat)};
            }
            @for bid in &flags.pinned_boats {
                input type="hidden" name="boat_pin" value=(bid);
            }
            @for bid in &flags.was_pinned_boats {
                input type="hidden" name="boat_was_pin" value=(bid);
            }
            @for bid in &flags.locked_boats {
                input type="hidden" name="boat_lock" value=(bid);
            }
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
        @let hidden = if eb.active { "false" } else { "true" };
        @let hide_style = if eb.active { "" } else { "display:none" };
        div class="border border-slate-200 rounded-lg overflow-hidden print-break"
             data-editor-boat=(boat.id)
             data-hidden=(hidden)
             style=(hide_style) {
            @let boat_pin_state = if flags.locked_boats.contains(&boat.id) {
                "locked"
            } else if flags.pinned_boats.contains(&boat.id) {
                "dirty"
            } else if flags.was_pinned_boats.contains(&boat.id) {
                "was_pinned"
            } else {
                "clean"
            };
            @let boat_icon_active = boat_pin_state == "locked" || boat_pin_state == "dirty";
            @let boat_icon_bg = if boat_icon_active { "bg-blue-100 rounded-full w-6 h-6 inline-flex items-center justify-center" } else { "" };
            @let in_use_team = flags.boats_in_use_by.get(&boat.id);
            @if let Some(team) = in_use_team {
                div class="bg-amber-50 border-b border-amber-200 px-4 py-1 text-xs font-medium text-amber-700" {
                    "In use by " (team)
                }
            }
            div class="bg-slate-100 px-4 py-2 border-b border-slate-200 flex items-center" {
                strong class="text-slate-800" { (boat.name) }
                span class="text-xs text-slate-500 ml-2" {
                    "(" (seat_count) "+"
                    ", " (rig_label(boat))
                    ")"
                }
                // Transfer button — select this boat to move rowers to another.
                button type="button"
                       class="ml-auto mr-2 text-xs text-slate-400 hover:text-blue-600 no-print"
                       ":class"={"selectedBoat === " (boat.id) " ? 'ml-auto mr-2 text-xs text-blue-600 font-semibold no-print' : 'ml-auto mr-2 text-xs text-slate-400 hover:text-blue-600 no-print'"}
                       title="Transfer rowers to another boat"
                       "@click.stop"={"selectBoatForTransfer(" (boat.id) ")"} {
                    "Transfer"
                }
                span {
                    @if boat_pin_state == "locked" {
                        button type="button"
                               class={"text-xs hover:text-violet-700 " (boat_icon_bg)}
                               title="Unlock boat"
                               "@click.stop"={"cycleBoatState('locked'," (boat.id) ")"} {
                            "\u{1F512}"
                        }
                    } @else if boat_pin_state == "dirty" {
                        button type="button"
                               class={"text-xs hover:text-amber-700 " (boat_icon_bg)}
                               title="Unpin boat"
                               "@click.stop"={"cycleBoatState('dirty'," (boat.id) ")"} {
                            "\u{1F4CC}"
                        }
                    } @else if boat_pin_state == "was_pinned" {
                        button type="button"
                               class="text-xs hover:text-violet-700 rotate-[-45deg] inline-block"
                               title="Lock boat"
                               "@click.stop"={"cycleBoatState('was_pinned'," (boat.id) ")"} {
                            "\u{1F4CC}"
                        }
                    } @else {
                        button type="button"
                               class="text-xs text-slate-300 hover:text-violet-700"
                               title="Lock boat"
                               "@click.stop"={"cycleBoatState('clean'," (boat.id) ")"} {
                            "\u{1F513}"
                        }
                    }
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
                        @let is_empty = maybe_rower.is_none();
                        // Determine seat pin state: locked > dirty > was_pinned > clean.
                        @let seat_triple = maybe_rower.map(|rid| (rid, boat.id, *seat));
                        @let pin_state = if seat_triple.map(|t| flags.locked_seats.contains(&t)).unwrap_or(false) {
                            "locked"
                        } else if seat_triple.map(|t| flags.pinned_seats.contains(&t)).unwrap_or(false) {
                            "dirty"
                        } else if seat_triple.map(|t| flags.was_pinned_seats.contains(&t)).unwrap_or(false) {
                            "was_pinned"
                        } else {
                            "clean"
                        };
                        @let row_base = if is_empty {
                            "border-b border-slate-100 last:border-0 cursor-pointer transition bg-slate-50"
                        } else if is_designated_cox {
                            "border-b border-slate-100 last:border-0 cursor-pointer transition border-l-4 border-l-indigo-400"
                        } else {
                            "border-b border-slate-100 last:border-0 cursor-pointer transition"
                        };
                        // Icon gets a blue background ring when the state influences generation.
                        @let icon_active = pin_state == "locked" || pin_state == "dirty";
                        @let seat_key = format!("{}:{}:{}", rower_id_str, boat.id, seat);
                        tr data-key=(key)
                           data-boat=(boat.id)
                           data-seat=(seat)
                           data-rower=(rower_id_str)
                           data-name=(rower_name)
                           data-stats=(stats_text)
                           data-pin-state=(pin_state)
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
                                @let icon_bg = if icon_active { "bg-blue-100 rounded-full w-6 h-6 inline-flex items-center justify-center" } else { "" };
                                td class="w-8 text-center lock-cell" {
                                    @if pin_state == "locked" {
                                        button type="button"
                                               class={"text-xs hover:text-violet-700 " (icon_bg)}
                                               title="Unlock"
                                               "@click.stop"={"cycleSeatState('locked','" (seat_key) "')"} {
                                            "\u{1F512}"
                                        }
                                    } @else if pin_state == "dirty" {
                                        button type="button"
                                               class={"text-xs hover:text-amber-700 " (icon_bg)}
                                               title="Unpin (let solver reassign)"
                                               "@click.stop"={"cycleSeatState('dirty','" (seat_key) "')"} {
                                            "\u{1F4CC}"
                                        }
                                    } @else if pin_state == "was_pinned" {
                                        button type="button"
                                               class="text-xs hover:text-violet-700 rotate-[-45deg] inline-block"
                                               title="Lock (keep this placement)"
                                               "@click.stop"={"cycleSeatState('was_pinned','" (seat_key) "')"} {
                                            "\u{1F4CC}"
                                        }
                                    } @else {
                                        button type="button"
                                               class="text-xs text-slate-300 hover:text-violet-700"
                                               title="Lock seat"
                                               "@click.stop"={"cycleSeatState('clean','" (seat_key) "')"} {
                                            "\u{1F513}"
                                        }
                                    }
                                }
                            } @else {
                                td class="w-8 lock-cell" {}
                            }
                            (side_indicator(rower))
                        }
                    }
                }
            }
        }
    }
}

/// Alpine JS for the lineup editor. Source lives in
/// `templates/js/lineup_editor.js` for IDE support.
fn editor_js() -> &'static str {
    include_str!("../js/lineup_editor.js")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineup_db::rower::types::{Side, SideStrength, SweepBias, RowerWeightClass, Skill, Strength, Height};
    use lineup_db::types::IntBool;

    fn rower_with_side(side: Side, strength: i32) -> Rower {
        Rower {
            id: RowerId::new(1),
            name: "Test".into(),
            weight_class: RowerWeightClass::Medium,
            skill: Skill::Intermediate,
            strength: Strength::Intermediate,
            height: Height::Medium,
            side,
            side_strength: SideStrength::new(strength),
            sweep_bias: SweepBias::SWEEP_HARD,
            can_cox: IntBool::new(false),
            is_designated_cox: IntBool::new(false),
            active: IntBool::new(true),
            created_at: chrono::NaiveDateTime::default(),
            updated_at: chrono::NaiveDateTime::default(),
        }
    }

    #[test]
    fn side_sort_key_ordering() {
        let port_hard = rower_with_side(Side::Port, 0);
        let port_flex = rower_with_side(Side::Port, 5);
        let either = rower_with_side(Side::Either, 0);
        let stbd_flex = rower_with_side(Side::Starboard, 5);
        let stbd_hard = rower_with_side(Side::Starboard, 0);

        let keys: Vec<i32> = vec![&port_hard, &port_flex, &either, &stbd_flex, &stbd_hard]
            .into_iter()
            .map(EditorData::side_sort_key)
            .collect();

        // Strictly ascending: port hard < port flex < either < stbd flex < stbd hard
        for w in keys.windows(2) {
            assert!(w[0] < w[1], "{} should be < {}", w[0], w[1]);
        }
    }

    #[test]
    fn side_sort_key_either_is_zero() {
        assert_eq!(EditorData::side_sort_key(&rower_with_side(Side::Either, 0)), 0);
        assert_eq!(EditorData::side_sort_key(&rower_with_side(Side::Either, 5)), 0);
    }

    #[test]
    fn side_sort_key_port_is_negative() {
        for s in 0..=5 {
            assert!(EditorData::side_sort_key(&rower_with_side(Side::Port, s)) < 0);
        }
    }

    #[test]
    fn side_sort_key_starboard_is_positive() {
        for s in 0..=5 {
            assert!(EditorData::side_sort_key(&rower_with_side(Side::Starboard, s)) > 0);
        }
    }
}
