//! Solve view template. Renders the solver's primary lineup, its
//! alternatives (Alpine-toggled), the unplaced-rowers breakdown, and
//! the commit button.

mod alternatives;
mod editor;
pub(crate) mod knobs;
pub(crate) mod profile_modal;

use editor::roster_pool;
pub(crate) use editor::{lineup_editor, roster_pool_oob, DisplayFlags, EditorData, OtherTeamRower};
use knobs::knobs_form;
pub(crate) use knobs::preset_bar;

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use lineup_db::{
    boat::{types::BoatId, Boat},
    practice::{Practice, PracticeId},
    rower::{types::RowerId, Rower},
    snapshot::DbSnapshot,
};
use maud::{html, Markup};

use crate::handlers::solve::{EditorTabsMeta, SolveKnobs};

// ── Shared helpers (used across submodules and/or by other templates) ──

/// Colored right-edge bar indicating side preference. Port = red,
/// starboard = green, Either = empty. Notches (gray lines from the
/// bottom) convey strength: more notches = weaker preference.
/// HARD/5 = solid, 4 = 1 notch, 3 = 2, 2 = 3, 1 = 4.
pub(crate) fn side_indicator(rower: Option<&Rower>) -> Markup {
    use lineup_db::rower::types::Side;
    let Some(r) = rower else {
        return html! { td class="w-2 p-0 side-cell" {} };
    };
    let color = match r.side {
        Side::Port => "bg-red-400",
        Side::Starboard => "bg-green-500",
        Side::Either => return html! { td class="w-2 p-0 side-cell" {} },
    };
    let notches = if r.side_strength.is_hard() {
        0
    } else {
        (5 - r.side_strength.as_int()).max(0)
    };
    if notches == 0 {
        return html! {
            td class={"w-2 p-0 side-cell " (color)} { "\u{00a0}" }
        };
    }
    // Notches centered vertically: use a repeating gradient sized to
    // the notch block height, positioned at center.
    let notch_h = 2; // px per gray line
    let gap = 3; // px between lines
    let block_h = notches * (notch_h + gap) - gap; // total notch block height
    let mut stops = Vec::new();
    for i in 0..notches {
        let start = i * (notch_h + gap);
        let end = start + notch_h;
        stops.push(format!(
            "#cbd5e1 {start}px,#cbd5e1 {end}px,transparent {end}px"
        ));
    }
    let gradient = format!(
        "background-image:linear-gradient(to bottom,{});background-size:100% {block_h}px;background-repeat:no-repeat;background-position:center",
        stops.join(",")
    );
    html! {
        td class={"w-2 p-0 side-cell " (color)} style=(gradient) { "\u{00a0}" }
    }
}

/// CSS class for a 1–4 ordinal tier.
fn tier_class(ordinal: i32) -> &'static str {
    match ordinal {
        1 => "stat-tier-1",
        2 => "stat-tier-2",
        3 => "stat-tier-3",
        _ => "stat-tier-4",
    }
}

/// Erg score info for a rower, if available.
pub(super) struct ErgInfo {
    /// 500m split display string, e.g. "1:42"
    pub split_label: String,
    /// Tooltip with full details
    pub tooltip: String,
    /// Tier for color (1-4 based on split quartiles)
    pub tier: i32,
}

impl ErgInfo {
    /// Build from erg scores, if this rower has one.
    pub fn from_scores(
        rower_id: lineup_db::rower::types::RowerId,
        scores: &lineup_db::snapshot::ErgScores,
    ) -> Option<Self> {
        let &time_cs = scores.times_cs.get(&rower_id)?;
        let dist = scores.distance_m;
        let split_label = lineup_db::erg_test::format_split_500(time_cs, dist);
        let full_time = lineup_db::erg_test::format_time_cs(time_cs);
        let dist_label = lineup_db::erg_test::format_distance(dist);
        let tooltip = format!("Erg: {split_label}/500m ({full_time} over {dist_label})");
        // Tier based on 500m split (lower = faster = higher tier).
        // Rough thresholds: <1:45 = tier 4, <1:55 = tier 3, <2:05 = tier 2, else tier 1.
        let split_cs = lineup_db::erg_test::split_500_cs(time_cs, dist);
        let split_secs = split_cs / 100;
        let tier = if split_secs < 105 {
            4
        } else if split_secs < 115 {
            3
        } else if split_secs < 125 {
            2
        } else {
            1
        };
        Some(Self {
            split_label,
            tooltip,
            tier,
        })
    }
}

/// CSS class for side preference badge coloring.
fn side_class(r: &Rower) -> &'static str {
    use lineup_db::rower::types::Side;
    match r.side {
        Side::Port => "stat-side-port",
        Side::Starboard => "stat-side-stbd",
        Side::Either => "stat-side-either",
    }
}

/// Compact stats line for a rower rendered as monospace badge chips.
/// Each badge has its own tooltip and is tinted by tier (1–4 ordinal)
/// so the color is consistent across categories. When an erg score
/// is available, it replaces the Strength badge.
pub(super) fn rower_stats_line_with_erg(
    r: &Rower,
    show_attributes: bool,
    erg_scores: Option<&lineup_db::snapshot::ErgScores>,
) -> Markup {
    let erg = erg_scores.and_then(|s| ErgInfo::from_scores(r.id, s));
    rower_stats_line_inner(r, show_attributes, erg.as_ref())
}

fn rower_stats_line_inner(r: &Rower, show_attributes: bool, erg: Option<&ErgInfo>) -> Markup {
    if show_attributes {
        let (weight_label, weight_tip, has_raw_weight) = if let Some(w) = r.weight_kg {
            let lbs = format!("{:.0}lb", w.to_lbs());
            let tip = format!(
                "Weight: {:.0} lb ({:.1} kg, {} class)",
                w.to_lbs(),
                w.as_f64(),
                r.weight_class
            );
            (lbs, tip, true)
        } else {
            let tip = format!("Weight: {} (no measurement on file)", r.weight_class);
            (r.weight_class.short().to_string(), tip, false)
        };
        let wt = tier_class(r.weight_class.ordinal());

        let (height_label, height_tip, has_raw_height) = if let Some(h) = r.height_m {
            let label = h.to_ft_in();
            let tip = format!("Height: {} ({:.2}m, {})", label, h.as_f64(), r.height);
            (label, tip, true)
        } else {
            let tip = format!("Height: {} (no measurement on file)", r.height);
            (format!("{}", r.height), tip, false)
        };
        let ht = tier_class(r.height.ordinal());

        let skill_tip = format!("Skill: {}", r.skill);
        let st = tier_class(r.skill.ordinal());

        let strength_tip = format!("Strength: {}", r.strength);
        let srt = tier_class(r.strength.ordinal());

        let side_tip = format!("Side: {}", compact_side(r));
        let sc = side_class(r);

        html! {
            div class="flex gap-1 mt-0.5 flex-wrap" {
                @if has_raw_weight {
                    span class={"stat-badge cursor-help " (wt)} title=(weight_tip) { (weight_label) }
                } @else {
                    span class={"stat-badge italic cursor-help " (wt)} title=(weight_tip) { (weight_label) }
                }
                span class={"stat-badge cursor-help " (st)} title=(skill_tip) { (r.skill.short()) }
                // Erg split replaces Strength when available
                @if let Some(e) = erg {
                    @let et = tier_class(e.tier);
                    @let erg_tip = format!("{}, Strength: {}", e.tooltip, r.strength);
                    span class={"stat-badge cursor-help " (et)} title=(erg_tip) { (e.split_label) }
                } @else {
                    span class={"stat-badge cursor-help " (srt)} title=(strength_tip) { (r.strength.short()) }
                }
                @if has_raw_height {
                    span class={"stat-badge cursor-help " (ht)} title=(height_tip) { (height_label) }
                } @else {
                    span class={"stat-badge italic cursor-help " (ht)} title=(height_tip) { (height_label) }
                }
                span class={"stat-badge cursor-help " (sc)} title=(side_tip) { (compact_side(r)) }
            }
        }
    } else {
        use lineup_db::rower::types::Side;
        if r.side != Side::Either {
            let side_tip = format!("Side: {}", compact_side(r));
            let sc = side_class(r);
            html! {
                div class="flex gap-1 mt-0.5" {
                    span class={"stat-badge cursor-help " (sc)} title=(side_tip) { (compact_side(r)) }
                }
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
pub(super) fn rig_label(b: &Boat) -> &'static str {
    use lineup_db::rower::types::Side;
    match b.stroke_side {
        Side::Port => "port-rigged",
        Side::Starboard => "starboard-rigged",
        Side::Either => "unrigged", // unreachable per SQL CHECK
    }
}

/// Compact side label with strength number for lineup cards.
/// e.g. "Port(-4)", "Stbd(+2)", "Either"
pub(super) fn compact_side(r: &Rower) -> String {
    use lineup_db::rower::types::Side;
    match r.side {
        Side::Either => "Either".to_string(),
        Side::Port => {
            let s = r.side_strength.as_int();
            let bias = if s == 0 { 5 } else { (6 - s).clamp(1, 5) };
            format!("Port({bias})")
        }
        Side::Starboard => {
            let s = r.side_strength.as_int();
            let bias = if s == 0 { 5 } else { (6 - s).clamp(1, 5) };
            format!("Starboard({bias})")
        }
    }
}

/// Whether the cox (seat 0) should be displayed first for this boat.
/// True when the tenant forces stern display or the boat is stern-loaded.
pub(super) fn cox_first(snapshot: &DbSnapshot, boat_id: BoatId, force_cox_stern: bool) -> bool {
    if force_cox_stern {
        return true;
    }
    snapshot
        .boats
        .iter()
        .find(|b| b.id == boat_id)
        .map(|b| b.cox_position.cox_first())
        .unwrap_or(true)
}

/// Sort seats for display: stern → bow. When `cox_first` is true,
/// cox (seat 0) comes before all numbered seats; otherwise it comes
/// after them.
pub(super) fn sort_seats_for_display(seats: &mut [(i32, RowerId)], cox_at_top: bool) {
    seats.sort_by_key(|(s, _)| {
        if *s == 0 {
            if cox_at_top {
                i32::MIN
            } else {
                i32::MAX
            }
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

/// Colored seat tag badge. Port = red tint, starboard = green tint,
/// cox = purple tint. Rectangular with side-color background.
pub(crate) fn seat_badge(boat: Option<&Boat>, seat: i32, label: &str) -> Markup {
    let side_class = if seat == 0 {
        "seat-tag seat-tag-cox"
    } else if let Some(b) = boat {
        use lineup_db::rower::types::Side;
        match b.seat_side(seat) {
            Some(Side::Port) => "seat-tag seat-tag-port",
            Some(Side::Starboard) => "seat-tag seat-tag-stbd",
            _ => "seat-tag",
        }
    } else {
        "seat-tag"
    };
    html! {
        span class=(side_class) {
            (label)
        }
    }
}

pub(super) fn find_rower(snapshot: &DbSnapshot, id: RowerId) -> Option<&Rower> {
    snapshot.rowers.iter().find(|r| r.id == id)
}

// ── Tab bar ──

fn tab_bar(meta: &EditorTabsMeta) -> Markup {
    html! {
        nav #tab-bar
            class="flex items-stretch gap-0 border-b no-print sticky top-0 z-10"
            style="min-height: 45px; border-color: var(--rule-2); background: var(--paper)" {
            @for tab in &meta.tabs {
                @let is_active = tab.id == meta.active;
                button class={
                    "tab-pill inline-flex items-center gap-1 px-3 text-sm font-medium transition border-b-2"
                    @if is_active { " text-ink border-ink" }
                    @else { " text-ink-3 border-transparent hover:text-ink hover:border-rule" }
                }
                data-tab-id=(tab.id)
                onclick=(format!("switchTab({})", tab.id)) {
                    span class="tab-label" { (tab.label) }
                    span class={"tab-close text-ink-3 hover:text-red-600 ml-1 text-xs" @if meta.tabs.len() <= 1 { " hidden" }}
                         onclick=(format!("event.stopPropagation(); removeTab({})", tab.id)) {
                        "\u{00d7}"
                    }
                }
            }
            button class="inline-flex items-center px-3 text-sm text-ink-3 hover:text-ink transition"
                   data-tab-add
                   onclick="addTab()" {
                "+ New"
            }
        }
    }
}

// ── Page-level content functions ──

/// Landing page before the solver runs. Shows knobs with a
/// "Generate" button (or "Re-generate" if lineups already exist),
/// plus a manual lineup builder with boat selection and an
/// available rower pool.
#[allow(clippy::too_many_arguments)]
pub(crate) fn landing_content(
    snapshot: &DbSnapshot,
    practice_id: PracticeId,
    date: NaiveDate,
    knobs: &SolveKnobs,
    committed_practices: &[Practice],
    has_committed: bool,
    custom_profiles: &[(String, Option<String>)],
    flags: &DisplayFlags,
    default_boats: &HashSet<BoatId>,
    draft_lineups: &[lineup_db::lineup::CommittedLineup],
    tab_meta: &EditorTabsMeta,
    _has_tabs_with_content: bool,
) -> Markup {
    let has_draft = !draft_lineups.is_empty();
    let available_count = snapshot.available_rowers().count();
    let subtitle = format!(
        "{available_count} members available · {boats} candidate shells",
        boats = snapshot.boats.len(),
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
                .map(|s| s.is_available())
                .unwrap_or(snapshot.assume_available)
        })
        .collect();

    // If seat params are present (e.g. from "Edit lineup" on history),
    // pre-populate the editor with those placements.
    // Otherwise, if drafts exist, load those into the editor.
    let editor = if !knobs.seat.is_empty() {
        let mut placements: HashMap<BoatId, HashMap<i32, RowerId>> = HashMap::new();
        for t in &knobs.seat {
            placements
                .entry(t.boat_id)
                .or_default()
                .insert(t.seat.as_int(), t.rower_id);
        }
        let active_boats: HashSet<BoatId> = knobs.boat.iter().copied().collect();
        EditorData::from_placements(snapshot, &placements, &active_boats)
    } else if has_draft {
        let mut placements: HashMap<BoatId, HashMap<i32, RowerId>> = HashMap::new();
        for cl in draft_lineups {
            let boat_seats = placements.entry(cl.lineup.boat_id).or_default();
            for seat in &cl.seats {
                boat_seats.insert(seat.seat_position.as_int(), seat.rower_id);
            }
        }
        let active_boats: HashSet<BoatId> = placements.keys().copied().collect();
        EditorData::from_placements(snapshot, &placements, &active_boats)
    } else {
        EditorData::empty(snapshot, default_boats)
    };

    html! {
        header class="border-b px-4 sm:px-8 py-3"
               style="border-color: var(--rule); background: var(--paper)" {
            div class="flex flex-wrap items-center justify-between gap-2" {
                div {
                    h1 class="text-xl font-bold font-serif-heading text-ink" {
                        "Set Lineups \u{00b7} " (date)
                        @if has_draft {
                            span class="ml-2 text-xs font-normal bg-amber-100 text-amber-800 px-1.5 py-0.5 rounded-full align-middle" {
                                "Draft"
                            }
                        }
                    }
                    p class="text-xs mt-0.5 text-muted" { (subtitle) }
                }
                div class="flex items-center gap-3 no-print" {
                    // Button group: Save draft + Clear
                    div class="inline-flex rounded-md shadow-soft" {
                        button type="submit" form="draft-form"
                               class="border border-rule rounded-l-md px-3 py-2 text-sm font-semibold text-ink-2 bg-paper hover:bg-paper-2 transition whitespace-nowrap" {
                            "Save draft"
                        }
                        button type="submit" form="clear-form"
                               class="border border-rule border-l-0 rounded-r-md px-3 py-2 text-sm font-semibold text-ink-3 bg-paper hover:bg-paper-2 transition whitespace-nowrap" {
                            "Clear"
                        }
                    }
                    // Commit — visually distinct, primary action
                    button type="submit" form="commit-form"
                           class="btn-accent font-semibold shadow transition whitespace-nowrap" {
                        "Commit lineup"
                    }
                }
            }
        }
        div class="solve-layout" x-data="lineupEditor()" {
            // Left sidebar: roster pool
            aside #roster-pool class="roster-sidebar" {
                (roster_pool(snapshot, practice_id, &editor, &unavailable, &knobs.walkon, &[]))
            }
            // Center: tab bar + editor
            div class="solve-center relative" {
                (tab_bar(tab_meta))
                // Selection hint — floats over the right side of the tab bar.
                // Zero height so it never shifts layout below.
                div class="sticky top-0 z-20 pointer-events-none no-print"
                     style="height: 0; transform: translateY(-45px); text-align: right"
                     x-show="selected || selectedBoat" x-cloak {
                    span class="pointer-events-auto inline-flex items-center px-3 text-xs text-accent whitespace-nowrap"
                         style="height: 43px; margin-top: 1px; background: var(--paper)"
                         x-show="selected && !selectedBoat"
                         x-cloak {
                        "Click another to swap \u{00b7} or click again to cancel"
                    }
                    span class="pointer-events-auto inline-flex items-center px-3 text-xs text-accent whitespace-nowrap"
                         style="height: 43px; margin-top: 1px; background: var(--paper)"
                         x-show="selectedBoat"
                         x-cloak {
                        "Click a boat pill to transfer rowers \u{00b7} or click Transfer again to cancel"
                    }
                }
                div class="px-4 sm:px-6 py-4 space-y-4" {
                    div #solve-results {
                        (lineup_editor(snapshot, practice_id, &editor, flags, has_draft))
                    }
                    // Hidden anchor for the SSE streaming skeleton.
                    // The generate form targets this instead of #solve-results
                    // so the editor stays visible during generation.
                    div #sse-anchor style="position:absolute;width:0;height:0;overflow:hidden" {}
                }
            }
            // Right rail: solver
            div class="no-print" style="border-left: 1px solid var(--rule); display: flex; flex-direction: column" {
                (knobs_form(practice_id, knobs, committed_practices, has_committed, custom_profiles, snapshot, None))
            }
        }
    }
}

/// Full page with streaming skeleton — used for direct browser navigation
/// to `/solve/{id}?generate=1`.
pub(crate) fn streaming_page(
    snapshot: &DbSnapshot,
    practice_id: PracticeId,
    date: NaiveDate,
    knobs: &SolveKnobs,
    committed_practices: &[Practice],
    custom_profiles: &[(String, Option<String>)],
) -> Markup {
    let available = snapshot.available_rowers().count();
    let subtitle = format!(
        "{available} members available · {boats} candidate shells",
        boats = snapshot.boats.len(),
    );

    // For the streaming page we need an empty editor to render the pool.
    let empty_editor = EditorData::empty(snapshot, &HashSet::new());

    html! {
        header class="border-b px-4 sm:px-8 py-3"
               style="border-color: var(--rule); background: var(--paper)" {
            h1 class="text-xl font-bold font-serif-heading text-ink" {
                "Set Lineups \u{00b7} " (date)
            }
            p class="text-xs mt-0.5 text-muted" { (subtitle) }
        }
        div class="solve-layout" x-data="lineupEditor()" {
            aside #roster-pool class="roster-sidebar" {
                (roster_pool(snapshot, practice_id, &empty_editor, &[], &knobs.walkon, &[]))
            }
            div class="solve-center" {
                div class="px-4 sm:px-6 py-4 space-y-4" {
                    div #solve-results {}
                    div #sse-anchor style="position:absolute;width:0;height:0;overflow:hidden" {
                        (streaming_skeleton(practice_id, knobs))
                    }
                }
            }
            // Right rail: solver
            div class="no-print" style="border-left: 1px solid var(--rule); display: flex; flex-direction: column" {
                (knobs_form(practice_id, knobs, committed_practices, true, custom_profiles, snapshot, None))
            }
        }
    }
}

/// Streaming skeleton: swapped into `#solve-results` when generate=1.
/// Contains the SSE connection that streams solver results.
pub(crate) fn streaming_skeleton(practice_id: PracticeId, knobs: &SolveKnobs) -> Markup {
    let sse_url = format!(
        "/solve/{practice_id}/stream?{}",
        serde_html_form::to_string(knobs).unwrap_or_default()
    );

    html! {
        div hx-ext="sse"
            "sse-connect"=(sse_url)
            "sse-close"="done" {

            // Primary result — replaced by the primary event.
            div "sse-swap"="primary"
                hx-swap="innerHTML"
                hx-disinherit="hx-ext" {}

            // Tab events — SSE delivers a <script> that calls
            // createTabFromSSE() to add an alternative as a new tab.
            div "sse-swap"="tab"
                hx-swap="innerHTML"
                hx-disinherit="hx-ext"
                class="hidden" {}

            // Done event — resets the generate button animation.
            div "sse-swap"="done"
                hx-swap="innerHTML"
                class="hidden" {}
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
    use test_case::test_case;

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
            weight_kg: None,
            height_m: None,
        }
    }

    // ── seat_label ──

    #[test_case(0, 8  => "cox"  ; "cox seat")]
    #[test_case(1, 8  => "bow"  ; "bow in eight")]
    #[test_case(8, 8  => "str"  ; "stroke in eight")]
    #[test_case(4, 8  => "s4"   ; "middle seat")]
    #[test_case(1, 1  => "bow"  ; "single seat is bow not stroke")]
    #[test_case(2, 2  => "str"  ; "stroke in pair")]
    #[test_case(4, 4  => "str"  ; "stroke in four")]
    #[test_case(1, 4  => "bow"  ; "bow in four")]
    #[test_case(3, 8  => "s3"   ; "s3 in eight")]
    fn seat_label_cases(seat: i32, seat_count: i32) -> String {
        seat_label(seat, seat_count)
    }

    // ── compact_side ──

    #[test]
    fn compact_side_either() {
        let r = rower_with_side(Side::Either, 3);
        assert_eq!(compact_side(&r), "Either");
    }

    #[test_case(Side::Port, 0 => "Port(5)"      ; "port hard lock")]
    #[test_case(Side::Port, 1 => "Port(5)"      ; "port strength 1")]
    #[test_case(Side::Port, 3 => "Port(3)"      ; "port strength 3")]
    #[test_case(Side::Port, 5 => "Port(1)"      ; "port flexible")]
    #[test_case(Side::Starboard, 0 => "Starboard(5)" ; "starboard hard lock")]
    #[test_case(Side::Starboard, 5 => "Starboard(1)" ; "starboard flexible")]
    fn compact_side_cases(side: Side, strength: i32) -> String {
        compact_side(&rower_with_side(side, strength))
    }
}
