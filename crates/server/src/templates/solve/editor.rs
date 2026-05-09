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
    compact_side, cox_first, find_rower, rig_label, rower_stats_line_with_erg, seat_badge,
    seat_label, side_indicator,
};

/// CSS inline style for a pool rower's side-colored right border.
fn pool_side_border_style(r: &Rower) -> &'static str {
    if r.is_designated_cox.as_bool() {
        "border-right: 2px solid var(--cox)"
    } else {
        match r.side {
            lineup_db::rower::types::Side::Port => "border-right: 2px solid var(--port)",
            lineup_db::rower::types::Side::Starboard => "border-right: 2px solid var(--stbd)",
            lineup_db::rower::types::Side::Either => "",
        }
    }
}

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
    /// Active oar sets available for assignment: (id, name, total_count).
    pub(crate) oar_sets: Vec<(lineup_db::oar_set::types::OarSetId, String, i32)>,
    /// Current oar assignments for this practice: boat_id → (oar_set_id, name).
    pub(crate) oar_assignments: HashMap<BoatId, (lineup_db::oar_set::types::OarSetId, String)>,
    /// Pre-computed: oar_set_id → total oars consumed across all boats in this practice.
    pub(crate) oar_usage: HashMap<lineup_db::oar_set::types::OarSetId, i32>,
}

impl DisplayFlags {
    /// Compute oar usage from assignments and the boat list.
    /// Only counts boats in `filled_boats` (boats that have at least one rower placed).
    pub(crate) fn compute_oar_usage(
        assignments: &HashMap<BoatId, (lineup_db::oar_set::types::OarSetId, String)>,
        boats: &[lineup_db::boat::Boat],
        filled_boats: &HashSet<BoatId>,
    ) -> HashMap<lineup_db::oar_set::types::OarSetId, i32> {
        let mut usage: HashMap<lineup_db::oar_set::types::OarSetId, i32> = HashMap::new();
        for (boat_id, (oar_set_id, _)) in assignments {
            if !filled_boats.contains(boat_id) {
                continue;
            }
            let oars_needed = boats
                .iter()
                .find(|b| b.id == *boat_id)
                .map(|b| b.seat_count.as_int() * b.oars_per_seat.as_int())
                .unwrap_or(0);
            *usage.entry(*oar_set_id).or_insert(0) += oars_needed;
        }
        usage
    }
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
            Side::Port => -(bias + 1), // -6..-1
            Side::Either => 0,
            Side::Starboard => bias + 1, // 1..6
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
        let boats = snapshot
            .boats
            .iter()
            .map(|b| {
                let has_cox = b.has_cox.as_bool();
                let mut seats = Vec::new();
                if has_cox {
                    seats.push((0, None));
                }
                for s in 1..=b.seat_count.as_int() {
                    seats.push((s, None));
                }
                let active = if use_defaults {
                    default_boats.contains(&b.id)
                } else {
                    true
                };
                EditorBoat {
                    boat: b,
                    seats,
                    active,
                }
            })
            .collect();
        let mut pool: Vec<&Rower> = snapshot.available_rowers().collect();
        Self::sort_pool(&mut pool);
        Self {
            boats,
            pool,
            sculling: vec![],
        }
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

        let boats = snapshot
            .boats
            .iter()
            .map(|b| {
                let lineup = primary.lineups.iter().find(|l| l.boat_id == b.id);
                let active = lineup.map(|l| l.used).unwrap_or(false);
                let seat_map = filled_map.get(&b.id);
                let has_cox = b.has_cox.as_bool();
                let mut seats = Vec::new();
                if has_cox {
                    seats.push((0, seat_map.and_then(|m| m.get(&0).copied())));
                }
                for s in 1..=b.seat_count.as_int() {
                    seats.push((s, seat_map.and_then(|m| m.get(&s).copied())));
                }
                EditorBoat {
                    boat: b,
                    seats,
                    active,
                }
            })
            .collect();

        let mut pool: Vec<&Rower> = primary
            .unplaced
            .benched
            .iter()
            .filter_map(|id| snapshot.rowers.iter().find(|r| r.id == *id))
            .collect();
        Self::sort_pool(&mut pool);

        Self {
            boats,
            pool,
            sculling: vec![],
        }
    }

    /// Build from explicit placements — used by the editor endpoint
    /// after client-side operations (swap, bench, toggle boat). The
    /// server re-renders the editor with correct indicators/stats.
    pub(crate) fn from_placements(
        snapshot: &'a DbSnapshot,
        placements: &HashMap<BoatId, HashMap<i32, RowerId>>,
        active_boats: &HashSet<BoatId>,
    ) -> Self {
        let boats = snapshot
            .boats
            .iter()
            .map(|b| {
                let active = active_boats.contains(&b.id);
                let seat_map = placements.get(&b.id);
                let has_cox = b.has_cox.as_bool();
                let mut seats = Vec::new();
                if has_cox {
                    seats.push((0, seat_map.and_then(|m| m.get(&0).copied())));
                }
                for s in 1..=b.seat_count.as_int() {
                    seats.push((s, seat_map.and_then(|m| m.get(&s).copied())));
                }
                EditorBoat {
                    boat: b,
                    seats,
                    active,
                }
            })
            .collect();

        // Pool = available rowers not placed in any seat.
        let placed: HashSet<RowerId> = placements
            .values()
            .flat_map(|m| m.values().copied())
            .collect();
        let mut pool: Vec<&Rower> = snapshot
            .available_rowers()
            .filter(|r| !placed.contains(&r.id))
            .collect();
        Self::sort_pool(&mut pool);

        Self {
            boats,
            pool,
            sculling: vec![],
        }
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
    _has_draft: bool,
) -> Markup {
    let commit_action = format!("/commit-lineup/{practice_id}");
    let draft_action = format!("/draft-lineup/{practice_id}");
    let clear_action = format!("/clear-draft/{practice_id}");
    let editor_url = format!("/solve/{practice_id}/editor");

    html! {
        // Hidden forms that carry seat data — buttons in the page header
        // reference these via the HTML5 `form` attribute.
        form #commit-form method="post" action=(commit_action) class="hidden" {
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
        }
        form #draft-form method="post" action=(draft_action)
             hx-post=(draft_action)
             hx-swap="none"
             "hx-on::after-request"="showSuccessToast('Draft saved')"
             class="hidden" {
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
        }
        form #clear-form method="post" action=(clear_action)
             hx-post=(clear_action)
             hx-target="#content"
             hx-push-url="true"
             class="hidden" {}

        section #lineup-editor class="solve-card pt-4 pb-4 px-4 sm:pb-6 sm:px-6"
               data-practice-id=(practice_id)
               data-editor-url=(editor_url) {

            // Boat selector chips
            div class="flex flex-wrap items-center gap-2 mb-4 no-print" {
                @for eb in &editor.boats {
                    @let bid = eb.boat.id.as_int();
                    @let active_class = if eb.active {
                        "boat-chip boat-chip-on"
                    } else {
                        "boat-chip"
                    };
                    @let pill_target_class = "boat-chip boat-chip-on";
                    @let filled = eb.seats.iter().filter(|(_, r)| r.is_some()).count();
                    @let total = eb.seats.len();
                    @let in_use_team = flags.boats_in_use_by.get(&eb.boat.id);
                    button type="button"
                           class=(active_class)
                           data-boat-id=(bid)
                           ":class"={"selectedBoat !== null && selectedBoat !== " (bid) " ? '" (pill_target_class) "' : '" (active_class) "'"}
                           "@click"={"boatPillClick(" (bid) ")"} {
                        span class="font-serif-heading text-sm text-ink" {
                            (eb.boat.name)
                        }
                        span class="font-mono-stat text-[10px] px-1 rounded"
                             style="border: 1px solid var(--rule); color: var(--muted)" {
                            (eb.boat.seat_count)
                            @if eb.boat.has_cox.as_bool() { "+" }
                        }
                        span class="font-mono-stat text-[10px] text-muted" {
                            (filled) "/" (total)
                        }
                        @if let Some(_team) = in_use_team {
                            span class="ml-1 inline-block px-1.5 py-0.5 text-[10px] font-semibold rounded-full leading-none"
                                 style="background: color-mix(in oklch, var(--warn) 20%, var(--paper)); color: var(--warn)" {
                                "In use"
                            }
                        }
                    }
                }
                @if editor.boats.len() > 2 {
                    span class="inline-flex items-center gap-1 ml-1" {
                        span class="text-xs text-rule" "aria-hidden"="true" { "\u{00b7}" }
                        button type="button"
                               class="px-2 py-1 text-xs font-medium font-mono-stat rounded cursor-pointer text-accent"
                               "@click"="selectAllBoats()" { "all" }
                        span class="text-xs text-rule" "aria-hidden"="true" { "\u{00b7}" }
                        button type="button"
                               class="px-2 py-1 text-xs font-medium font-mono-stat rounded cursor-pointer text-accent"
                               "@click"="deselectAllBoats()" { "none" }
                    }
                }
                @if !flags.oar_sets.is_empty() {
                    @let active_boat_ids: Vec<String> = editor.boats.iter()
                        .filter(|eb| eb.active)
                        .map(|eb| eb.boat.id.to_string())
                        .collect();
                    span class="inline-flex items-center gap-1 ml-1 no-print" {
                        span class="text-xs text-rule" "aria-hidden"="true" { "\u{00b7}" }
                        form class="inline" hx-post="/oars/auto-assign" hx-swap="none"
                             "hx-on::after-request"="oarAutoAssignDone()" {
                            input type="hidden" name="practice_id" value=(practice_id);
                            @for bid in &active_boat_ids {
                                input type="hidden" name="boat_ids" value=(bid);
                            }
                            button type="submit"
                                   class="px-2 py-1 text-xs font-medium font-mono-stat rounded cursor-pointer"
                                   style="color: var(--ink-2)" {
                                "auto-assign oars"
                            }
                        }
                    }
                    script { (maud::PreEscaped("
                        function oarAutoAssignDone() {
                            var ed = document.getElementById('lineup-editor');
                            if (ed) {
                                var d = Alpine.$data(ed);
                                if (d && d.gatherState) {
                                    htmx.ajax('GET', ed.dataset.editorUrl + '?' + d.gatherState(), {target: ed, swap: 'outerHTML'});
                                }
                            }
                        }
                    ")) }
                }
            }

            // Boat cards (all rendered; inactive ones hidden via style)
            div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4" {
                @for eb in &editor.boats {
                    (editor_boat_card(snapshot, practice_id, eb, flags))
                }
            }

        }

        // Alpine editor logic is in the global layout script
        // (lineup_editor.js) so it's defined before Alpine init.

        // Pin/lock/boat state lives in #editor-knob-state inside the
        // knobs form. JS manages it via _addKnobInput/_removeKnobInput;
        // gatherState() round-trips it as query params on each rerender.
        // No OOB swap — JS is the sole owner after initial page load.
    }
}

/// Render one boat card in the unified editor.
fn editor_boat_card(
    snapshot: &DbSnapshot,
    practice_id: PracticeId,
    eb: &EditorBoat,
    flags: &DisplayFlags,
) -> Markup {
    let boat = eb.boat;
    let seat_count = boat.seat_count.as_int();
    let cox_at_top = cox_first(snapshot, boat.id, flags.force_cox_stern);
    let mut seats = eb.seats.clone();
    seats.sort_by_key(|(s, _)| {
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
        @let hidden = if eb.active { "false" } else { "true" };
        @let hide_style = if eb.active { "" } else { "display:none" };
        div class="solve-card overflow-hidden print-break"
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
            @let in_use_team = flags.boats_in_use_by.get(&boat.id);
            @if let Some(team) = in_use_team {
                div class="px-4 py-1 text-xs font-medium"
                    style="background: color-mix(in oklch, var(--warn) 15%, var(--paper)); border-bottom: 1px solid var(--rule-2); color: var(--warn)" {
                    "In use by " (team)
                }
            }
            // Card header
            div class="px-4 py-3 flex items-center gap-3"
                style="border-bottom: 1px dashed var(--rule)" {
                div class="flex-1 min-w-0" {
                    h3 class="font-serif-heading font-medium text-base text-ink" {
                        (boat.name)
                    }
                    div class="font-mono-stat text-[10px] flex flex-wrap gap-1 mt-0.5 text-muted" {
                        span class="px-1 rounded" style="border: 1px solid var(--rule); color: var(--ink-2); background: var(--paper-2)" {
                            (seat_count)
                            @if boat.has_cox.as_bool() { "+" }
                        }
                        span "aria-hidden"="true" { "\u{00b7}" }
                        span { (boat.weight_class) }
                        span "aria-hidden"="true" { "\u{00b7}" }
                        span class="inline-flex items-center gap-0.5" {
                            (crate::templates::layout::rigger_icon(boat.stroke_side))
                            (rig_label(boat))
                        }
                        span "aria-hidden"="true" { "\u{00b7}" }
                        @let filled = seats.iter().filter(|(_, r)| r.is_some()).count();
                        span class="inline-flex items-center gap-0.5" {
                            (crate::templates::layout::seat_icon())
                            (filled) "/" (seats.len())
                        }
                    }
                    @if !flags.oar_sets.is_empty() {
                        @let current_oar = flags.oar_assignments.get(&boat.id);
                        @let oar_label = current_oar.map(|(_, name)| name.as_str()).unwrap_or("—");
                        @let current_over = current_oar
                            .and_then(|(oid, _)| {
                                let total = flags.oar_sets.iter().find(|(id, _, _)| id == oid).map(|(_, _, c)| *c).unwrap_or(0);
                                let used = flags.oar_usage.get(oid).copied().unwrap_or(0);
                                if used > total { Some(()) } else { None }
                            })
                            .is_some();
                        button type="button"
                               class="font-mono-stat text-[10px] mt-1 no-print px-1.5 py-0.5 rounded cursor-pointer border"
                               style=(if current_over {
                                   "border-color: var(--warn); color: var(--warn); background: color-mix(in oklch, var(--warn) 8%, var(--paper))"
                               } else if current_oar.is_some() {
                                   "border-color: var(--rule); color: var(--ink-2); background: var(--paper-2)"
                               } else {
                                   "border-color: var(--rule-2); color: var(--muted); border-style: dashed"
                               })
                               hx-get=(format!("/oars/pick?practice_id={}&boat_id={}", practice_id, boat.id))
                               hx-target="body"
                               hx-swap="beforeend" {
                            (crate::templates::layout::crossed_oars_icon())
                            @if current_over { (oar_label) " !" } @else { (oar_label) }
                        }
                    }
                }
                // Transfer button
                button type="button"
                       class="text-xs no-print cursor-pointer text-muted"
                       ":class"={"selectedBoat === " (boat.id) " ? 'text-xs no-print cursor-pointer font-semibold' : 'text-xs no-print cursor-pointer text-muted'"}
                       ":style"={"selectedBoat === " (boat.id) " ? 'color: var(--accent)' : 'color: var(--muted)'"}
                       title="Transfer rowers to another boat"
                       "@click.stop"={"selectBoatForTransfer(" (boat.id) ")"} {
                    "Transfer"
                }
                // Lock/pin icon
                span {
                    @if boat_pin_state == "locked" {
                        button type="button"
                               class="lock-btn lock-btn-on"
                               title="Boat locked \u{2014} all seats kept when generating. Click to unlock."
                               "aria-label"="Unlock boat"
                               "aria-pressed"="true"
                               "@click.stop"={"cycleBoatState('locked'," (boat.id) ")"} {
                            span "aria-hidden"="true" { "\u{25CF}" }
                        }
                    } @else if boat_pin_state == "dirty" {
                        button type="button"
                               class="lock-btn lock-btn-dirty"
                               title="Boat pinned \u{2014} honored on next generate. Click to unpin."
                               "aria-label"="Unpin boat"
                               "aria-pressed"="true"
                               "@click.stop"={"cycleBoatState('dirty'," (boat.id) ")"} {
                            span "aria-hidden"="true" { "\u{25CF}" }
                        }
                    } @else if boat_pin_state == "was_pinned" {
                        button type="button"
                               class="lock-btn lock-btn-was-pinned"
                               title="Boat was pinned last run, now free. Click to lock."
                               "aria-label"="Lock boat"
                               "aria-pressed"="false"
                               "@click.stop"={"cycleBoatState('was_pinned'," (boat.id) ")"} {
                            span "aria-hidden"="true" { "\u{25CB}" }
                        }
                    } @else {
                        button type="button"
                               class="lock-btn"
                               title="Click to lock this boat"
                               "aria-label"="Lock boat"
                               "aria-pressed"="false"
                               "@click.stop"={"cycleBoatState('clean'," (boat.id) ")"} {
                            span "aria-hidden"="true" { "\u{25CB}" }
                        }
                    }
                }
            }
            // Seat rows
            @let natural_order = !boat.has_cox.as_bool() || boat.cox_position.cox_first() == cox_at_top;
            div class="px-2 py-1" {
                // Stern end cap — only when display order matches the boat's real layout
                @if natural_order {
                    div class="end-cap" {
                        span class="end-cap-rule" {}
                        span { "stern" }
                        span class="end-cap-rule" {}
                    }
                }
                @for (seat, maybe_rower) in &seats {
                    @let key = format!("{}:{}", boat.id, seat);
                    @let label = seat_label(*seat, seat_count);
                    @let rower = maybe_rower.and_then(|id| find_rower(snapshot, id));
                    @let rower_id_str = maybe_rower.map(|id| id.as_int().to_string()).unwrap_or_default();
                    @let rower_name = rower.map(|r| r.display_name()).unwrap_or_default();
                    @let stats_text = rower.map(|r| format!(
                        "{} · {} · {} · {}",
                        r.weight_class.short(), r.skill.short(), r.strength.short(), compact_side(r)
                    )).unwrap_or_default();
                    @let is_empty = maybe_rower.is_none();
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
                    @let seat_key = format!("{}:{}:{}", rower_id_str, boat.id, seat);
                    @let seat_aria = if is_empty {
                        format!("{}: empty", label)
                    } else {
                        format!("{}: {}", label, rower_name)
                    };
                    // Grid row: seat-tag | rower-cell | lock | side-indicator
                    div data-key=(key)
                        data-boat=(boat.id)
                        data-seat=(seat)
                        data-rower=(rower_id_str)
                        data-name=(rower_name)
                        data-stats=(stats_text)
                        data-pin-state=(pin_state)
                        role="button"
                        tabindex="0"
                        "aria-label"=(seat_aria)
                        class="grid gap-1 my-0.5 rounded cursor-pointer transition items-center"
                        style={"grid-template-columns: 44px 1fr auto 28px 8px; border: 1px " (if is_empty { "dashed var(--rule-2)" } else { "solid color-mix(in oklch, var(--rule-2) 50%, transparent)" })}
                        ":style"={"selected === '" (key) "' ? 'grid-template-columns: 44px 1fr auto 28px 8px; background: color-mix(in oklch, var(--accent) 10%, var(--paper)); border: 1px solid var(--accent)' : 'grid-template-columns: 44px 1fr auto 28px 8px; border: 1px " (if is_empty { "dashed var(--rule-2)" } else { "solid color-mix(in oklch, var(--rule-2) 50%, transparent)" }) "'"}
                        "@click"={"select('" (key) "')"}
                        "@keydown.enter"={"select('" (key) "')"}
                        "@keydown.space.prevent"={"select('" (key) "')"} {
                        // Seat tag
                        div class="flex items-center justify-center" {
                            (seat_badge(Some(boat), *seat, &label))
                        }
                        // Rower content
                        div class="px-2 min-w-0 flex items-center" style="min-height: 48px; padding-top: 2px; padding-bottom: 5px" {
                            @if let Some(r) = rower {
                                div class="w-full" {
                                    span class="font-medium font-serif-heading text-sm text-ink" { (r.display_name()) }
                                    (rower_stats_line_with_erg(r, flags.show_attributes, snapshot.erg_scores.as_ref()))
                                }
                            } @else if is_empty {
                                span class="font-mono-stat text-xs italic text-muted" { "\u{2014} empty \u{2014}" }
                            } @else {
                                span class="font-mono-stat text-xs italic text-muted" { "unknown" }
                            }
                        }
                        // Commit meter
                        div class="sm:pr-2" {
                            @if let Some(r) = rower {
                                (super::commit_meter(r))
                            }
                        }
                        // Lock button
                        @if !is_empty {
                            div class="lock-cell" {
                                @if pin_state == "locked" {
                                    button type="button"
                                           class="lock-btn lock-btn-on"
                                           title="Locked \u{2014} always kept when generating. Click to unlock."
                                           "aria-label"="Unlock seat"
                                           "aria-pressed"="true"
                                           "@click.stop"={"cycleSeatState('locked','" (seat_key) "')"} {
                                        span "aria-hidden"="true" { "\u{25CF}" }
                                    }
                                } @else if pin_state == "dirty" {
                                    button type="button"
                                           class="lock-btn lock-btn-dirty"
                                           title="Pinned \u{2014} honored on next generate. Click to unpin."
                                           "aria-label"="Unpin seat"
                                           "aria-pressed"="true"
                                           "@click.stop"={"cycleSeatState('dirty','" (seat_key) "')"} {
                                        span "aria-hidden"="true" { "\u{25CF}" }
                                    }
                                } @else if pin_state == "was_pinned" {
                                    button type="button"
                                           class="lock-btn lock-btn-was-pinned"
                                           title="Was pinned last run, now free. Click to lock."
                                           "aria-label"="Lock seat"
                                           "aria-pressed"="false"
                                           "@click.stop"={"cycleSeatState('was_pinned','" (seat_key) "')"} {
                                        span "aria-hidden"="true" { "\u{25CB}" }
                                    }
                                } @else {
                                    button type="button"
                                           class="lock-btn"
                                           title="Click to lock this seat"
                                           "aria-label"="Lock seat"
                                           "aria-pressed"="false"
                                           "@click.stop"={"cycleSeatState('clean','" (seat_key) "')"} {
                                        span "aria-hidden"="true" { "\u{25CB}" }
                                    }
                                }
                            }
                        } @else {
                            div class="lock-cell" {}
                        }
                        (side_indicator(rower))
                    }
                }
                // Bow end cap — only when display order matches the boat's real layout
                @if natural_order {
                    div class="end-cap" {
                        span class="end-cap-rule" {}
                        span { "bow" }
                        span class="end-cap-rule" {}
                    }
                }
            }
        }
    }
}

/// Render a single rower row in the pool sidebar.
fn pool_rower_row(r: &Rower, boat_kind: &str, snapshot: &DbSnapshot) -> Markup {
    let key = format!("{boat_kind}:{}", r.id);
    let side_style = pool_side_border_style(r);
    html! {
        span data-key=(key)
             data-boat=(boat_kind)
             data-seat="-1"
             data-rower=(r.id)
             role="button"
             tabindex="0"
             "aria-label"={"Available: " (r.display_name())}
             class="px-2 py-1 rounded cursor-pointer transition"
             style={"border: 1px solid transparent; " (side_style)}
             ":style"={"selected === '" (key) "' ? 'border: 1px solid var(--accent); background: color-mix(in oklch, var(--accent) 10%, var(--paper)); " (side_style) "' : 'border: 1px solid transparent; " (side_style) "'"}
             "@click"={"select('" (key) "')"}
             "@keydown.enter"={"select('" (key) "')"}
             "@keydown.space.prevent"={"select('" (key) "')"} {
            div {
                div class="float-right ml-1.5" { (super::commit_meter(r)) }
                span class="font-medium text-xs text-ink" { (r.display_name()) }
            }
            (rower_stats_line_with_erg(r, true, snapshot.erg_scores.as_ref()))
        }
    }
}

// ── Roster pool sidebar ──────────────────────────────────────────

/// Render the roster pool sidebar content. Used both for initial page
/// render and for OOB swaps when the editor re-renders.
pub(crate) fn roster_pool(
    snapshot: &DbSnapshot,
    practice_id: PracticeId,
    editor: &EditorData,
    unavailable: &[&Rower],
    walkon_ids: &[lineup_db::rower::types::RowerId],
    other_team_rowers: &[OtherTeamRower],
) -> Markup {
    let pool_count = editor.pool.len();
    let total_rowers = snapshot
        .rowers
        .iter()
        .filter(|r| r.active.as_bool())
        .count();
    let has_boated = editor
        .boats
        .iter()
        .any(|eb| eb.active && eb.seats.iter().any(|(_, r)| r.is_some()));

    html! {
        // Header
        div class="rp-head" {
            h2 class="font-serif-heading font-medium text-base m-0 text-ink" { "Roster" }
            span class="font-mono-stat text-[10px] uppercase tracking-wide text-muted" {
                (pool_count) " avail \u{00b7} " (total_rowers) " total"
            }
        }

        // Walk-on section
        @let has_addable = unavailable.iter().any(|r| !walkon_ids.contains(&r.id));
        @if has_addable || !walkon_ids.is_empty() {
            div class="px-4 py-2 text-sm" style="border-bottom: 1px solid var(--rule-2)" {
                div class="flex items-center gap-2 flex-wrap" {
                    @if !walkon_ids.is_empty() {
                        @for id in walkon_ids {
                            @let name = snapshot.rowers.iter().find(|r| r.id == *id).map(|r| r.display_name()).unwrap_or_else(|| "?".to_string());
                            span class="inline-block px-2 py-0.5 text-xs rounded-full"
                                 style="background: color-mix(in oklch, var(--good) 15%, var(--paper)); color: var(--good)" {
                                (name)
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
                            input type="hidden" name="partial" value="0";
                            @for w in walkon_ids {
                                input type="hidden" name="walkon" value=(w);
                            }
                            select name="walkon"
                                   class="rounded px-2 py-1.5 text-xs focus:outline-none"
                                   style="border: 1px solid var(--rule); background: var(--paper-2); color: var(--ink)" {
                                @for r in unavailable {
                                    @if !walkon_ids.contains(&r.id) {
                                        option value=(r.id) { (r.display_name()) }
                                    }
                                }
                            }
                            button type="submit"
                                   class="text-xs font-semibold uppercase tracking-wide cursor-pointer text-accent" {
                                "+ Walk-on"
                            }
                        }
                    }
                }
            }
        }

        // Rower list
        div class="rp-section-list" {
            // Sculling section
            @if !editor.sculling.is_empty() {
                div class="mt-2" {
                    div class="pool-section-head" {
                        span class="font-serif-heading italic font-medium text-xs text-ink-2" { "Sculling" }
                        span class="font-mono-stat text-[10px] text-muted" { (editor.sculling.len()) }
                    }
                    div class="flex flex-col gap-0.5 mt-1" {
                        @for r in &editor.sculling {
                            (pool_rower_row(r, "sculling", snapshot))
                        }
                    }
                }
            }

            // Bench slot
            @if has_boated {
                div class="mt-2" {
                    span data-key="bench:empty"
                         data-boat="bench"
                         data-seat="-1"
                         data-rower=""
                         role="button"
                         tabindex="0"
                         "aria-label"="Bench slot"
                         class="block px-2 py-1.5 rounded cursor-pointer transition text-center"
                         style="border: 1px dashed var(--rule)"
                         ":style"={"selected === 'bench:empty' ? 'border: 1px solid var(--accent); background: color-mix(in oklch, var(--accent) 10%, var(--paper))' : 'border: 1px dashed var(--rule)'"}
                         "@click"="select('bench:empty')"
                         "@keydown.enter"="select('bench:empty')"
                         "@keydown.space.prevent"="select('bench:empty')" {
                        span class="font-mono-stat italic text-xs text-muted" { "\u{2014} bench \u{2014}" }
                    }
                }
            }

            // Available rowers — grouped by side
            @if !editor.pool.is_empty() {
                @let coxes: Vec<_> = editor.pool.iter().filter(|r| r.is_designated_cox.as_bool()).collect();
                @let ports: Vec<_> = editor.pool.iter().filter(|r| !r.is_designated_cox.as_bool() && r.side == lineup_db::rower::types::Side::Port).collect();
                @let stbds: Vec<_> = editor.pool.iter().filter(|r| !r.is_designated_cox.as_bool() && r.side == lineup_db::rower::types::Side::Starboard).collect();
                @let eithers: Vec<_> = editor.pool.iter().filter(|r| !r.is_designated_cox.as_bool() && r.side == lineup_db::rower::types::Side::Either).collect();

                @if !coxes.is_empty() {
                    div class="mt-2" {
                        div class="pool-section-head" {
                            span class="font-serif-heading italic font-medium text-xs text-cox" { "Coxswains" }
                            span class="font-mono-stat text-[10px] text-muted" { (coxes.len()) }
                        }
                        div class="flex flex-col gap-0.5 mt-1" {
                            @for r in &coxes {
                                (pool_rower_row(r, "bench", snapshot))
                            }
                        }
                    }
                }
                @if !ports.is_empty() {
                    div class="mt-2" {
                        div class="pool-section-head" {
                            span class="font-serif-heading italic font-medium text-xs text-port" { "Port" }
                            span class="font-mono-stat text-[10px] text-muted" { (ports.len()) }
                        }
                        div class="flex flex-col gap-0.5 mt-1" {
                            @for r in &ports {
                                (pool_rower_row(r, "bench", snapshot))
                            }
                        }
                    }
                }
                @if !stbds.is_empty() {
                    div class="mt-2" {
                        div class="pool-section-head" {
                            span class="font-serif-heading italic font-medium text-xs text-stbd" { "Starboard" }
                            span class="font-mono-stat text-[10px] text-muted" { (stbds.len()) }
                        }
                        div class="flex flex-col gap-0.5 mt-1" {
                            @for r in &stbds {
                                (pool_rower_row(r, "bench", snapshot))
                            }
                        }
                    }
                }
                @if !eithers.is_empty() {
                    div class="mt-2" {
                        div class="pool-section-head" {
                            span class="font-serif-heading italic font-medium text-xs text-either" { "Either" }
                            span class="font-mono-stat text-[10px] text-muted" { (eithers.len()) }
                        }
                        div class="flex flex-col gap-0.5 mt-1" {
                            @for r in &eithers {
                                (pool_rower_row(r, "bench", snapshot))
                            }
                        }
                    }
                }
            }

            // Other team rowers
            @if !other_team_rowers.is_empty() {
                div class="mt-3" {
                    div class="pool-section-head" {
                        span class="font-serif-heading italic font-medium text-xs text-warn" { "Other teams" }
                    }
                    div class="flex flex-col gap-0.5 mt-1" {
                        @for otr in other_team_rowers {
                            @let r = &otr.rower;
                            @let side_style = pool_side_border_style(r);
                            span class="px-2 py-1 rounded"
                                 style={"border: 1px solid transparent; background: color-mix(in oklch, var(--warn) 5%, var(--paper)); " (side_style)} {
                                div class="font-medium text-xs text-ink" {
                                    (r.display_name())
                                    span class="ml-1 text-[9px] font-normal text-warn" { "(" (&otr.team_name) ")" }
                                }
                                (rower_stats_line_with_erg(r, true, snapshot.erg_scores.as_ref()))
                            }
                        }
                    }
                }
            }
        }
    }
}

/// OOB wrapper for roster_pool — used when the editor re-renders.
/// Returns an `<aside>` with `hx-swap-oob="outerHTML"` that replaces
/// the sidebar pool in place.
pub(crate) fn roster_pool_oob(
    snapshot: &DbSnapshot,
    practice_id: PracticeId,
    editor: &EditorData,
    unavailable: &[&Rower],
    walkon_ids: &[lineup_db::rower::types::RowerId],
    other_team_rowers: &[OtherTeamRower],
) -> Markup {
    html! {
        aside #roster-pool class="roster-sidebar" hx-swap-oob="outerHTML" {
            (roster_pool(snapshot, practice_id, editor, unavailable, walkon_ids, other_team_rowers))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineup_db::rower::types::{
        Height, RowerWeightClass, Side, SideStrength, Skill, Strength, SweepBias,
    };
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
            first_name: None,
            last_name: None,
            active: IntBool::new(true),
            created_at: chrono::NaiveDateTime::default(),
            updated_at: chrono::NaiveDateTime::default(),
            weight_kg: None,
            height_m: None,
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
        assert_eq!(
            EditorData::side_sort_key(&rower_with_side(Side::Either, 0)),
            0
        );
        assert_eq!(
            EditorData::side_sort_key(&rower_with_side(Side::Either, 5)),
            0
        );
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
