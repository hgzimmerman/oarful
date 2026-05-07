//! Attendance grid — horizontal scrollable table with rowers as rows
//! and practice dates as columns. Color-coded cells show availability
//! status using the warm paper palette.

use std::collections::HashMap;

use chrono::NaiveDate;
use lineup_db::availability::types::AvailabilityStatus;
use lineup_db::practice::PracticeId;
use lineup_db::rower::types::RowerId;
use lineup_db::rower::Rower;
use maud::{html, Markup, PreEscaped};

/// A practice column in the grid, carrying both date and ID.
pub(crate) struct PracticeColumn {
    pub(crate) id: PracticeId,
    pub(crate) date: NaiveDate,
    pub(crate) committed: bool,
}

pub(crate) fn grid_content(
    rowers: &[Rower],
    columns: &[PracticeColumn],
    avail_map: &HashMap<(RowerId, PracticeId), AvailabilityStatus>,
    show_past: bool,
    today: NaiveDate,
    editable: bool,
) -> Markup {
    // Compute attendance stats.
    let yes_count = avail_map
        .values()
        .filter(|s| **s == AvailabilityStatus::Yes)
        .count();
    let no_count = avail_map
        .values()
        .filter(|s| **s == AvailabilityStatus::No)
        .count();
    let responded = yes_count + no_count;
    let rate_pct = if responded > 0 {
        (yes_count as f64 / responded as f64 * 100.0).round() as u32
    } else {
        0
    };

    html! {
        // ── Header ──
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            div class="flex items-center justify-between" {
                div {
                    h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" {
                        "Attendance"
                    }
                    p class="font-mono-stat text-xs tracking-wide mt-1" style="color: var(--muted)" {
                        (rowers.len()) " members · " (columns.len()) " practices"
                        @if show_past { " (incl. past year)" }
                    }
                }
                div {
                    @if show_past {
                        a href="/team/attendance"
                          hx-get="/team/attendance"
                          hx-target="#team-tab-content"
                          hx-push-url="true"
                          class="btn-warm-ghost text-xs py-2" {
                            "Future only"
                        }
                    } @else {
                        a href="/team/attendance?show_past=1"
                          hx-get="/team/attendance?show_past=1"
                          hx-target="#team-tab-content"
                          hx-push-url="true"
                          class="btn-warm-ghost text-xs py-2" {
                            "Show past year"
                        }
                    }
                }
            }
        }

        // ── Summary strip ──
        @if !columns.is_empty() && !rowers.is_empty() {
            div class="flex items-stretch gap-0 px-4 sm:px-8 py-3 border-b flex-wrap" style="border-color: var(--rule); background: var(--paper)" {
                div class="flex items-baseline gap-3 pr-6" {
                    span class="cv-stat-num font-serif-heading" { (rowers.len()) }
                    span class="font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "members" }
                }
                div class="cv-stat-sep" {}
                div class="flex items-baseline gap-3 pr-6" {
                    span class="cv-stat-num font-serif-heading" { (columns.len()) }
                    span class="font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "practices" }
                }
                div class="cv-stat-sep" {}
                div class="flex items-baseline gap-3 pr-6" {
                    span class="cv-stat-num font-serif-heading" { (rate_pct) "%" }
                    span class="font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "attendance" }
                }
            }
        }

        div class="px-4 sm:px-8 py-6" {
            @if columns.is_empty() {
                div class="text-center italic py-12 font-mono-stat text-xs" style="color: var(--muted)" {
                    "No practices scheduled."
                }
            } @else if rowers.is_empty() {
                div class="text-center italic py-12 font-mono-stat text-xs" style="color: var(--muted)" {
                    "No roster members."
                }
            } @else {
                div class="overflow-auto rounded-lg max-h-[75vh]" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                    table class="text-xs border-collapse" {
                        caption class="sr-only" { "Attendance" }
                        thead {
                            tr {
                                th scope="col" class="att-corner" { "Rower" }
                                @for col in columns {
                                    (date_header(&col.date, today, if col.committed { Some(&col.id) } else { None }))
                                }
                            }
                        }
                        tbody {
                            @for rower in rowers {
                                tr class="att-row" {
                                    th scope="row" class="att-name" {
                                        (rower.display_name())
                                    }
                                    @for col in columns {
                                        @let status = avail_map.get(&(rower.id, col.id));
                                        @if editable {
                                            (editable_status_cell(status, rower.id, col.id))
                                        } @else {
                                            (status_cell(status))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Legend
                div class="flex items-center gap-5 mt-4 font-mono-stat text-[10px] tracking-wide uppercase" style="color: var(--muted)" {
                    span class="flex items-center gap-1.5" {
                        span class="att-legend-swatch att-yes" {}
                        "Present"
                    }
                    span class="flex items-center gap-1.5" {
                        span class="att-legend-swatch att-no" {}
                        "Absent"
                    }
                    span class="flex items-center gap-1.5" {
                        span class="att-legend-swatch" style="background: var(--paper-2); border: 1px solid var(--rule)" {}
                        "No response"
                    }
                }

                @if editable {
                    (tap_guard_script())
                }
            }
        }
    }
}

fn date_header(date: &NaiveDate, today: NaiveDate, practice_id: Option<&PracticeId>) -> Markup {
    let is_today = *date == today;
    let class = if is_today {
        "att-date-head att-today"
    } else {
        "att-date-head"
    };
    let full_date = date.format("%A, %B %-d, %Y").to_string();

    html! {
        th scope="col" class=(class) title=(full_date) {
            @if let Some(pid) = practice_id {
                a href=(format!("/history/{pid}"))
                  hx-get=(format!("/history/{pid}"))
                  hx-target="#content"
                  hx-push-url="true"
                  class="hover:opacity-80" style="color: var(--link)" {
                    div class="text-[9px] uppercase tracking-wider" { (date.format("%a")) }
                    div { (date.format("%b")) }
                    div class="text-sm font-bold" { (date.format("%-d")) }
                }
            } @else {
                div class="text-[9px] uppercase tracking-wider" style="color: var(--muted)" { (date.format("%a")) }
                div style="color: var(--ink-3)" { (date.format("%b")) }
                div class="text-sm font-bold" style="color: var(--ink-2)" { (date.format("%-d")) }
            }
        }
    }
}

fn status_cell(status: Option<&AvailabilityStatus>) -> Markup {
    let (class, title) = match status {
        Some(AvailabilityStatus::Yes) => ("att-cell att-yes", "Present"),
        Some(AvailabilityStatus::No) => ("att-cell att-no", "Absent"),
        None => ("att-cell", "No response"),
    };
    html! {
        td class=(class) title=(title) {}
    }
}

fn editable_status_cell(
    status: Option<&AvailabilityStatus>,
    rower_id: RowerId,
    practice_id: PracticeId,
) -> Markup {
    let (class, title, next) = match status {
        Some(AvailabilityStatus::Yes) => (
            "att-cell att-yes cursor-pointer select-none",
            "Present → click: Absent",
            "No",
        ),
        Some(AvailabilityStatus::No) => (
            "att-cell att-no cursor-pointer select-none",
            "Absent → click: Clear",
            "clear",
        ),
        None => (
            "att-cell cursor-pointer select-none",
            "No response → click: Present",
            "Yes",
        ),
    };
    html! {
        td class=(class)
           title=(title)
           id=(format!("c-{rower_id}-{practice_id}"))
           data-rower=(rower_id.to_string())
           data-practice=(practice_id.to_string())
           data-next=(next)
           hx-post="/team/attendance/toggle"
           hx-vals=(format!(r#"{{"rower_id":"{}","practice_id":"{}","status":"{}"}}"#, rower_id, practice_id, next))
           hx-target=(format!("#c-{rower_id}-{practice_id}"))
           hx-swap="outerHTML"
           {}
    }
}

/// Public entry point for the handler to render a single replacement cell.
pub(crate) fn editable_status_cell_markup(
    status: Option<&AvailabilityStatus>,
    rower_id: RowerId,
    practice_id: PracticeId,
) -> Markup {
    editable_status_cell(status, rower_id, practice_id)
}

/// Inline script that suppresses HTMX clicks when the user is swiping
/// (finger moved > 10px from touchstart). Prevents accidental toggles
/// on mobile while scrolling the grid.
fn tap_guard_script() -> Markup {
    html! {
        script {
            (PreEscaped(r#"
(function(){
  var sx=0,sy=0;
  document.addEventListener('touchstart',function(e){
    sx=e.touches[0].clientX; sy=e.touches[0].clientY;
  },{passive:true});
  document.addEventListener('touchend',function(e){
    var dx=e.changedTouches[0].clientX-sx;
    var dy=e.changedTouches[0].clientY-sy;
    if(Math.abs(dx)>10||Math.abs(dy)>10){
      e.target.setAttribute('data-swiped','1');
    }
  },{passive:true});
  document.body.addEventListener('htmx:confirm',function(e){
    var el=e.target;
    if(el.hasAttribute('data-swiped')){
      el.removeAttribute('data-swiped');
      e.preventDefault();
    }
  });
})();
"#))
        }
    }
}
