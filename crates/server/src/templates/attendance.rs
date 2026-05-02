//! Attendance grid — horizontal scrollable table with rowers as rows
//! and practice dates as columns. Color-coded: green = present,
//! red = absent, white = no response.

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
    let subtitle = format!(
        "{} members · {} practices{}",
        rowers.len(),
        columns.len(),
        if show_past { " (incl. past year)" } else { "" },
    );

    html! {
        header class="bg-paper border-b border-rule-2 px-4 sm:px-8 py-4 sm:py-6" {
            div class="flex items-center justify-between" {
                div {
                    h1 class="text-2xl font-bold text-ink" { "Attendance" }
                    p class="text-sm text-ink-3 mt-1" { (subtitle) }
                }
                div {
                    @if show_past {
                        a href="/team/attendance"
                          hx-get="/team/attendance"
                          hx-target="#team-tab-content"
                          hx-push-url="true"
                          class="text-sm font-semibold text-ink-2 border border-rule px-3 py-1.5 rounded transition hover:bg-paper-2" {
                            "Future only"
                        }
                    } @else {
                        a href="/team/attendance?show_past=1"
                          hx-get="/team/attendance?show_past=1"
                          hx-target="#team-tab-content"
                          hx-push-url="true"
                          class="text-sm font-semibold text-ink-2 border border-rule px-3 py-1.5 rounded transition hover:bg-paper-2" {
                            "Show past year"
                        }
                    }
                }
            }
        }

        div class="px-4 sm:px-8 py-6" {
            @if columns.is_empty() {
                div class="text-center text-ink-3 italic py-12" {
                    "No practices scheduled."
                }
            } @else if rowers.is_empty() {
                div class="text-center text-ink-3 italic py-12" {
                    "No roster members."
                }
            } @else {
                div class="overflow-auto bg-paper rounded-lg shadow-soft max-h-[75vh]" {
                    table class="text-xs border-collapse" {
                        caption class="sr-only" { "Attendance" }
                        thead {
                            tr {
                                th scope="col" class="sticky top-0 left-0 z-20 bg-paper-2 px-3 py-2 text-left font-semibold text-ink-2 border-b border-r border-rule-2 min-w-[140px]" {
                                    "Rower"
                                }
                                @for col in columns {
                                    (date_header(&col.date, today, if col.committed { Some(&col.id) } else { None }))
                                }
                            }
                        }
                        tbody {
                            @for rower in rowers {
                                tr {
                                    th scope="row" class="sticky left-0 z-10 bg-paper px-3 py-1.5 font-medium text-ink border-b border-r border-rule-2 whitespace-nowrap text-left" {
                                        (rower.name)
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
                div class="flex items-center gap-4 mt-3 text-xs text-ink-3" {
                    span class="flex items-center gap-1" {
                        span class="inline-block w-3 h-3 rounded-sm bg-emerald-400" {}
                        "Present"
                    }
                    span class="flex items-center gap-1" {
                        span class="inline-block w-3 h-3 rounded-sm bg-red-400" {}
                        "Absent"
                    }
                    span class="flex items-center gap-1" {
                        span class="inline-block w-3 h-3 rounded-sm border border-rule-2" {}
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
    let bg = if is_today { "bg-blue-50" } else { "bg-paper-2" };
    let base_class = format!("sticky top-0 z-10 {bg} px-1.5 py-2 text-center font-medium border-b border-rule-2 whitespace-nowrap min-w-[44px]");
    let full_date = date.format("%A, %B %-d, %Y").to_string();

    html! {
        th scope="col" class=(base_class) title=(full_date) {
            @if let Some(pid) = practice_id {
                a href=(format!("/history/{pid}"))
                  hx-get=(format!("/history/{pid}"))
                  hx-target="#content"
                  hx-push-url="true"
                  class="text-link hover:text-link-2" {
                    div class="text-[10px] uppercase" { (date.format("%a")) }
                    div { (date.format("%b")) }
                    div class="text-sm font-bold" { (date.format("%-d")) }
                }
            } @else {
                div class="text-[10px] text-muted uppercase" { (date.format("%a")) }
                div class="text-ink-2" { (date.format("%b")) }
                div class="text-sm font-bold text-ink-2" { (date.format("%-d")) }
            }
        }
    }
}

fn status_cell(status: Option<&AvailabilityStatus>) -> Markup {
    let (bg, title) = match status {
        Some(AvailabilityStatus::Yes) => ("bg-emerald-400", "Present"),
        Some(AvailabilityStatus::No) => ("bg-red-400", "Absent"),
        None => ("", "No response"),
    };
    html! {
        td class=(format!("{bg} border-b border-r border-rule-2 min-w-[44px] h-7"))
           title=(title) {}
    }
}

fn editable_status_cell(
    status: Option<&AvailabilityStatus>,
    rower_id: RowerId,
    practice_id: PracticeId,
) -> Markup {
    let (bg, title, next) = match status {
        Some(AvailabilityStatus::Yes) => ("bg-emerald-400", "Present → click: Absent", "No"),
        Some(AvailabilityStatus::No) => ("bg-red-400", "Absent → click: Clear", "clear"),
        None => ("", "No response → click: Present", "Yes"),
    };
    html! {
        td class=(format!("{bg} border-b border-r border-rule-2 min-w-[44px] h-7 cursor-pointer select-none"))
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
