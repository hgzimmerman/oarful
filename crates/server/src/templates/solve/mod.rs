//! Solve view template. Renders the solver's primary lineup, its
//! alternatives (Alpine-toggled), the unplaced-rowers breakdown, and
//! the commit button.

mod alternatives;
mod editor;
pub(crate) mod knobs;
pub(crate) mod profile_modal;

pub(crate) use alternatives::alternative_block as stream_alternative_block;
pub(crate) use editor::{lineup_editor, DisplayFlags, EditorData, OtherTeamRower};
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

use crate::handlers::solve::SolveKnobs;

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

/// Compact stats line for a rower rendered as monospace badge chips.
/// When `show_attributes` is false, only shows side preference
/// (non-sensitive); otherwise shows the full breakdown.
///
/// Shows actual weight (lbs) and height (ft'in") when the rower has
/// real measurements on file, otherwise falls back to the bucket
/// abbreviation. A single tooltip on the whole row shows all stats
/// in a human-readable format.
pub(super) fn rower_stats_line(r: &Rower, show_attributes: bool) -> Markup {
    if show_attributes {
        // Weight: show real value when available, else bucket
        let weight_label = if let Some(w) = r.weight_kg {
            format!("{:.0}lb", w.to_lbs())
        } else {
            r.weight_class.short().to_string()
        };
        // Height: show real value when available, else bucket abbreviation
        let height_label = if let Some(h) = r.height_m {
            h.to_ft_in()
        } else {
            format!("{}", r.height)
        };
        let has_raw_weight = r.weight_kg.is_some();
        let has_raw_height = r.height_m.is_some();

        // Build a single human-readable tooltip for the whole stats row
        let mut tip_parts = Vec::new();

        // Weight
        if let Some(w) = r.weight_kg {
            tip_parts.push(format!(
                "Weight: {:.0} lb ({:.1} kg, {})",
                w.to_lbs(),
                w.as_f64(),
                r.weight_class
            ));
        } else {
            tip_parts.push(format!(
                "Weight: {} (no measurement on file)",
                r.weight_class
            ));
        }

        // Height
        if let Some(h) = r.height_m {
            tip_parts.push(format!(
                "Height: {} ({:.2}m, {})",
                h.to_ft_in(),
                h.as_f64(),
                r.height
            ));
        } else {
            tip_parts.push(format!("Height: {} (no measurement on file)", r.height));
        }

        tip_parts.push(format!("Skill: {}", r.skill));
        tip_parts.push(format!("Strength: {}", r.strength));
        tip_parts.push(format!("Side: {}", compact_side(r)));

        let tooltip = tip_parts.join(" \u{2022} ");

        html! {
            div class="flex gap-1 mt-0.5 flex-wrap cursor-help" title=(tooltip) {
                @if has_raw_weight {
                    span class="stat-badge" { (weight_label) }
                } @else {
                    span class="stat-badge italic" { (weight_label) }
                }
                span class="stat-badge" { (r.skill.short()) }
                span class="stat-badge" { (r.strength.short()) }
                @if has_raw_height {
                    span class="stat-badge" { (height_label) }
                } @else {
                    span class="stat-badge italic" { (height_label) }
                }
                span class="stat-badge" { (compact_side(r)) }
            }
        }
    } else {
        use lineup_db::rower::types::Side;
        if r.side != Side::Either {
            html! {
                div class="flex gap-1 mt-0.5" {
                    span class="stat-badge" { (compact_side(r)) }
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

// ── Page-level content functions ──

/// Landing page before the solver runs. Shows knobs with a
/// "Generate" button (or "Re-generate" if lineups already exist),
/// plus a manual lineup builder with boat selection and an
/// available rower pool.
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
) -> Markup {
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
    } else {
        EditorData::empty(snapshot, default_boats)
    };

    html! {
        div class="solve-page" {
            header class="border-b px-4 sm:px-8 py-4 sm:py-6"
                   style="border-color: var(--rule); background: var(--paper)" {
                h1 class="text-2xl font-bold font-serif-heading"
                   style="color: var(--ink)" {
                    "Set Lineups \u{00b7} " (date)
                }
                p class="text-sm mt-1" style="color: var(--muted)" { (subtitle) }
            }
            div class="px-4 sm:px-8 py-6 space-y-6 max-w-6xl mx-auto" {
                div class="no-print" {
                    (knobs_form(practice_id, knobs, committed_practices, has_committed, custom_profiles, snapshot, None))
                }
                div #solve-results {
                    (lineup_editor(snapshot, practice_id, &editor, flags, &unavailable, &knobs.walkon, &[]))
                }
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

    html! {
        div class="solve-page" {
            header class="border-b px-4 sm:px-8 py-4 sm:py-6"
                   style="border-color: var(--rule); background: var(--paper)" {
                h1 class="text-2xl font-bold font-serif-heading"
                   style="color: var(--ink)" {
                    "Set Lineups \u{00b7} " (date)
                }
                p class="text-sm mt-1" style="color: var(--muted)" { (subtitle) }
            }
            div class="px-4 sm:px-8 py-6 space-y-6 max-w-6xl mx-auto" {
                div class="no-print" {
                    (knobs_form(practice_id, knobs, committed_practices, true, custom_profiles, snapshot, None))
                }
                div #solve-results {
                    (streaming_skeleton(practice_id, knobs))
                }
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

            // Error placeholder — shown if primary fails.
            div "sse-swap"="error"
                hx-swap="innerHTML" {}

            // Alternatives container — each alternative is appended.
            div "sse-swap"="alternative"
                hx-swap="beforeend"
                hx-disinherit="hx-ext"
                class="space-y-4 mt-6" {}

            // Single spinner at the bottom — pushed down as results
            // stream in above it. Replaced by elapsed time on "done".
            div "sse-swap"="done"
                hx-swap="innerHTML" {
                div class="flex items-center justify-center gap-2 py-8 text-slate-400 text-sm" {
                    div class="inline-block w-5 h-5 border-2 border-slate-200 border-t-slate-500 rounded-full animate-spin" {}
                    "Generating..."
                }
            }
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
