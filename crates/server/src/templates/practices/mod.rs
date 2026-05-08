//! Practices dashboard templates: Planning tab, Committed tab, shared types.
//!
//! Email preview modals live in submodules:
//! - [`reminder_modal`] — reminder preview + confirm
//! - [`lineup_modal`] — lineup preview + confirm

mod lineup_modal;
mod reminder_modal;
mod send_result_modal;

pub(crate) use lineup_modal::{lineup_preview_modal, AlsoReadyPractice, LineupRecipientPreview};
pub(crate) use reminder_modal::{reminder_preview_modal, ReminderRecipientPreview};
pub(crate) use send_result_modal::{
    send_result_billing_gate, send_result_modal, SendResultRecipient, SendStatus,
};

use lineup_db::practice::{PracticeId, PracticePhase, PracticeWithPhase};
use lineup_db::types::DurationMinutes;
use maud::{html, Markup};

use super::layout::{empty_state, page_header};

// ── Unified practice list (phase-based) ──────────────────────────────

/// Full unified practice list page — replaces the tabbed Planning/Committed view.
#[allow(clippy::too_many_arguments)]
pub(crate) fn unified_page(
    practices: &[PracticeWithPhase],
    is_coach: bool,
    assume_available: bool,
    today: chrono::NaiveDate,
    default_time: Option<chrono::NaiveTime>,
    default_duration: Option<DurationMinutes>,
    suggested_date: Option<chrono::NaiveDate>,
) -> Markup {
    let min_date = today.format("%Y-%m-%d").to_string();
    let date_value = suggested_date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let time_value = default_time
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default();
    let end_time_value = match (default_time, default_duration) {
        (Some(t), Some(dur)) => {
            let end = t + chrono::TimeDelta::minutes(dur.as_int() as i64);
            end.format("%H:%M").to_string()
        }
        _ => String::new(),
    };

    // Split into upcoming and past/complete.
    let upcoming: Vec<_> = practices
        .iter()
        .filter(|p| p.practice.date >= today || !matches!(p.phase, PracticePhase::Complete))
        .collect();
    let past: Vec<_> = practices
        .iter()
        .filter(|p| p.practice.date < today && matches!(p.phase, PracticePhase::Complete))
        .collect();

    let ready_count = upcoming
        .iter()
        .filter(|p| matches!(p.phase, PracticePhase::Ready))
        .count();

    html! {
        (page_header("Practices", None))
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
            // Create practice form (coach only)
            @if is_coach {
                form method="post" action="/practices"
                     hx-post="/practices"
                     hx-target="#content"
                     hx-push-url="true" {
                  fieldset class="flex items-end gap-3 flex-wrap mb-2" style="border: none; padding: 0; margin: 0" {
                    legend class="sr-only" { "Add practice" }
                    div {
                        label for="date" class="block text-xs font-semibold text-ink-2 uppercase tracking-wide mb-1" {
                            "Add practice"
                        }
                        input id="date" name="date" type="date"
                              min=(min_date)
                              value=(date_value)
                              class="border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                    }
                    div {
                        label for="time" class="block text-xs font-semibold text-ink-2 uppercase tracking-wide mb-1" {
                            "Start"
                        }
                        input id="time" name="time" type="time"
                              value=(time_value)
                              class="border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                    }
                    div {
                        label for="end_time" class="block text-xs font-semibold text-ink-2 uppercase tracking-wide mb-1" {
                            "End"
                        }
                        input id="end_time" name="end_time" type="time"
                              value=(end_time_value)
                              class="border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                    }
                    button type="submit"
                           class="btn-accent font-semibold shadow-soft transition text-sm" {
                        "Create"
                    }
                  }
                }
            }

            // Bulk send button
            @if is_coach && ready_count > 0 {
                @let ready_dates_query: String = upcoming.iter()
                    .filter(|p| matches!(p.phase, PracticePhase::Ready) && !p.is_stale)
                    .map(|p| format!("dates={}", p.practice.date.format("%Y-%m-%d")))
                    .collect::<Vec<_>>()
                    .join("&");
                div class="flex justify-end" {
                    button type="button"
                           hx-get={"/practices/lineup-preview?" (ready_dates_query)}
                           hx-target="body"
                           hx-swap="beforeend"
                           class="btn-warm-ink py-2 px-4 text-sm" {
                        "Send all ready (" (ready_count) ")"
                    }
                }
            }

            // Upcoming practices
            @if upcoming.is_empty() {
                (empty_state("No upcoming practices. Create one above to get started."))
            } @else {
                div class="bg-paper rounded-lg shadow-soft divide-y divide-rule-2" {
                    @for pwp in &upcoming {
                        (unified_row(pwp, is_coach, assume_available))
                    }
                }
            }

            // Past / complete
            @if !past.is_empty() {
                div class="mt-8" {
                    h3 class="text-xs font-semibold text-ink-3 uppercase tracking-wide mb-3" {
                        "Past practices"
                    }
                    div class="bg-paper rounded-lg shadow-soft divide-y divide-rule-2 opacity-75" {
                        @for pwp in &past {
                            (unified_row(pwp, is_coach, assume_available))
                        }
                    }
                }
            }
        }
    }
}

fn phase_badge(phase: PracticePhase, is_stale: bool) -> Markup {
    let (dot_color, label) = if is_stale {
        ("var(--warn)", "Stale lineup")
    } else {
        match phase {
            PracticePhase::Created => ("var(--muted)", phase.label()),
            PracticePhase::Committed => ("var(--accent)", phase.label()),
            PracticePhase::Ready => ("var(--good)", phase.label()),
            PracticePhase::Notified => ("var(--good)", phase.label()),
            PracticePhase::Complete => ("var(--muted)", phase.label()),
            PracticePhase::Cancelled => ("var(--bad)", phase.label()),
        }
    };
    html! {
        span class="inline-flex items-center gap-1.5 text-xs font-semibold" {
            span class="inline-block w-2 h-2 rounded-full"
                 style={"background: " (dot_color)} {}
            span style={"color: " (dot_color)} { (label) }
        }
    }
}

fn primary_action(pwp: &PracticeWithPhase, assume_available: bool) -> Option<Markup> {
    let pid = pwp.practice.id;
    if pwp.is_stale && !matches!(pwp.phase, PracticePhase::Created | PracticePhase::Cancelled) {
        let href = format!("/solve/{pid}");
        return Some(html! {
            a href=(href)
              hx-get=(href)
              hx-target="#content"
              hx-push-url="true"
              class="btn-warm-ink text-xs py-1.5 px-3" {
                "Fix lineup"
            }
        });
    }
    match pwp.phase {
        PracticePhase::Created => {
            // If assume_available is off and most rowers haven't responded,
            // the primary action is sending reminders rather than generating.
            let needs_reminders =
                !assume_available && pwp.non_respondent_count > 0 && pwp.yes_count < 4;
            if needs_reminders {
                Some(html! {
                    button type="button"
                           hx-get={"/practices/reminder-preview?practice_ids=" (pid)}
                           hx-target="body"
                           hx-swap="beforeend"
                           class="btn-warm-ink text-xs py-1.5 px-3" {
                        "Send reminders"
                    }
                })
            } else {
                let href = format!("/solve/{pid}");
                Some(html! {
                    a href=(href)
                      hx-get=(href)
                      hx-target="#content"
                      hx-push-url="true"
                      class="btn-warm-ink text-xs py-1.5 px-3" {
                        "Generate lineup"
                    }
                })
            }
        }
        PracticePhase::Committed => {
            let href = format!("/history/{pid}");
            Some(html! {
                a href=(href)
                  hx-get=(href)
                  hx-target="#content"
                  hx-push-url="true"
                  class="btn-warm-ink text-xs py-1.5 px-3" {
                    "Build plan"
                }
            })
        }
        PracticePhase::Ready => Some(html! {
            button type="button"
                   hx-get={"/practices/lineup-preview?practice_id=" (pid) "&scope=practice"}
                   hx-target="body"
                   hx-swap="beforeend"
                   class="btn-warm-ink text-xs py-1.5 px-3" {
                "Send lineups"
            }
        }),
        PracticePhase::Notified | PracticePhase::Complete | PracticePhase::Cancelled => None,
    }
}

fn unified_row(pwp: &PracticeWithPhase, is_coach: bool, assume_available: bool) -> Markup {
    let p = &pwp.practice;
    let weekday = p.date.format("%A").to_string();
    let opacity = if p.cancelled.as_bool() {
        " opacity-40"
    } else {
        ""
    };

    // Link target depends on phase.
    let href = match pwp.phase {
        PracticePhase::Created => {
            if is_coach {
                Some(format!("/solve/{}", p.id))
            } else {
                None
            }
        }
        _ => Some(format!("/history/{}", p.id)),
    };

    html! {
        div class={"flex items-center justify-between px-6 py-3 hover:bg-paper-2 transition" (opacity)} {
            // Left side: phase badge + date/time + stats
            div class="flex-1 min-w-0" {
                @if let Some(ref href) = href {
                    a href=(href)
                      hx-get=(href)
                      hx-target="#content"
                      hx-push-url="true"
                      class="block cursor-pointer" {
                        (row_info(pwp, &weekday))
                    }
                } @else {
                    (row_info(pwp, &weekday))
                }
            }

            // Right side: primary action + secondary actions
            @if is_coach {
                div class="flex items-center gap-2 ml-3 shrink-0" {
                    @if let Some(action) = primary_action(pwp, assume_available) {
                        (action)
                    }
                    @if !matches!(pwp.phase, PracticePhase::Complete) {
                        (secondary_actions(pwp, assume_available))
                    }
                }
            }
        }
    }
}

fn row_info(pwp: &PracticeWithPhase, weekday: &str) -> Markup {
    let p = &pwp.practice;
    html! {
        div class="flex items-center gap-2 mb-0.5" {
            (phase_badge(pwp.phase, pwp.is_stale))
            span class="font-semibold text-ink text-sm" { (p.date) }
            span class="text-sm text-ink-3" { (weekday) }
            @if let Some(t) = p.time {
                span class="text-sm text-ink-3" {
                    " · " (t.format("%-I:%M %p"))
                }
            }
        }
        div class="flex items-center gap-3 text-xs text-ink-3" {
            @if pwp.yes_count > 0 || pwp.total_responses > 0 {
                span class="text-emerald-700 font-semibold" {
                    (pwp.yes_count) " available"
                }
            }
            @if pwp.boat_count > 0 {
                span { (pwp.boat_count) " boat(s)" }
            }
            @if pwp.non_respondent_count > 0 {
                span class="text-amber-700" {
                    (pwp.non_respondent_count) " pending"
                }
            }
            @if pwp.unnotified_rower_count > 0 && !matches!(pwp.phase, PracticePhase::Created) {
                span class="text-amber-700" {
                    (pwp.unnotified_rower_count) " not notified"
                }
            }
        }
    }
}

fn secondary_actions(pwp: &PracticeWithPhase, assume_available: bool) -> Markup {
    let pid = pwp.practice.id;
    let cancel_action = format!("/practices/{pid}/cancel");
    let is_cancelled = pwp.practice.cancelled.as_bool();

    html! {
        div class="relative" "x-data"="{ open: false }" {
            button type="button"
                   "@click"="open = !open"
                   class="text-muted hover:text-ink text-sm px-1 py-1 rounded" {
                "···"
            }
            div class="absolute right-0 top-full mt-1 bg-paper border border-rule rounded shadow-soft py-1 min-w-[140px] z-10"
                "x-show"="open"
                "@click.outside"="open = false"
                "x-transition" {
                @match pwp.phase {
                    PracticePhase::Created => {
                        // Show whichever action isn't the primary.
                        @let needs_reminders = !assume_available
                            && pwp.non_respondent_count > 0
                            && pwp.yes_count < 4;
                        @if needs_reminders {
                            // Primary is "Send reminders", so secondary is "Generate lineup".
                            a href={"/solve/" (pid)}
                              hx-get={"/solve/" (pid)}
                              hx-target="#content"
                              hx-push-url="true"
                              class="block w-full text-left px-3 py-2 text-sm hover:bg-paper-2" {
                                "Generate lineup"
                            }
                        } @else if pwp.non_respondent_count > 0 {
                            // Primary is "Generate lineup", so secondary is "Send reminders".
                            button type="button"
                                   hx-get={"/practices/reminder-preview?practice_ids=" (pid)}
                                   hx-target="body"
                                   hx-swap="beforeend"
                                   "@click"="open = false"
                                   class="block w-full text-left px-3 py-2 text-sm hover:bg-paper-2" {
                                "Send reminders"
                            }
                        }
                    }
                    PracticePhase::Committed => {
                        @if !pwp.is_stale {
                            button type="button"
                                   hx-get={"/practices/lineup-preview?practice_id=" (pid) "&scope=practice"}
                                   hx-target="body"
                                   hx-swap="beforeend"
                                   "@click"="open = false"
                                   class="block w-full text-left px-3 py-2 text-sm hover:bg-paper-2" {
                                "Send without plan"
                            }
                        }
                        a href={"/solve/" (pid)}
                          hx-get={"/solve/" (pid)}
                          hx-target="#content"
                          hx-push-url="true"
                          class="block w-full text-left px-3 py-2 text-sm hover:bg-paper-2" {
                            "Edit lineup"
                        }
                    }
                    PracticePhase::Ready => {
                        a href={"/history/" (pid)}
                          hx-get={"/history/" (pid)}
                          hx-target="#content"
                          hx-push-url="true"
                          class="block w-full text-left px-3 py-2 text-sm hover:bg-paper-2" {
                            "Edit plan"
                        }
                        a href={"/solve/" (pid)}
                          hx-get={"/solve/" (pid)}
                          hx-target="#content"
                          hx-push-url="true"
                          class="block w-full text-left px-3 py-2 text-sm hover:bg-paper-2" {
                            "Edit lineup"
                        }
                    }
                    PracticePhase::Notified => {
                        @if !pwp.is_stale {
                            button type="button"
                                   hx-get={"/practices/lineup-preview?practice_id=" (pid) "&scope=practice"}
                                   hx-target="body"
                                   hx-swap="beforeend"
                                   "@click"="open = false"
                                   class="block w-full text-left px-3 py-2 text-sm hover:bg-paper-2" {
                                "Re-send lineups"
                            }
                        }
                        a href={"/solve/" (pid)}
                          hx-get={"/solve/" (pid)}
                          hx-target="#content"
                          hx-push-url="true"
                          class="block w-full text-left px-3 py-2 text-sm hover:bg-paper-2" {
                            "Edit lineup"
                        }
                    }
                    PracticePhase::Cancelled | PracticePhase::Complete => {}
                }
                // Cancel / Restore — available in all non-complete phases
                @if !matches!(pwp.phase, PracticePhase::Complete) {
                    div class="border-t border-rule my-1" {}
                    form method="post" action=(cancel_action)
                         hx-post=(cancel_action)
                         hx-target="body"
                         hx-swap="beforeend" {
                        button type="submit"
                               "@click"="open = false"
                               class="block w-full text-left px-3 py-2 text-sm hover:bg-paper-2 text-red-600" {
                            @if is_cancelled { "Restore" } @else { "Cancel" }
                        }
                    }
                }
            }
        }
    }
}

// ── Cancel confirmation modal ────────────────────────────────────────

const CANCEL_CLOSE_JS: &str = "releaseFocus(); document.getElementById('cancel-modal').remove(); document.getElementById('cancel-modal-backdrop').remove()";

pub(crate) fn cancel_confirm_modal(practice_id: PracticeId) -> Markup {
    let silent_url = format!("/practices/{practice_id}/cancel-silent");
    let notify_url = format!("/practices/{practice_id}/cancel-notify");

    html! {
        div id="cancel-modal-backdrop"
            class="fixed inset-0 bg-black/40 z-40"
            onclick=(CANCEL_CLOSE_JS) {}
        div id="cancel-modal"
            role="dialog"
            "aria-modal"="true"
            "aria-label"="Cancel practice"
            class="fixed inset-0 z-50 flex items-start justify-center pt-12 px-4 pointer-events-none" {
            div class="bg-paper rounded-lg shadow-xl w-full max-w-md pointer-events-auto" {
                div class="px-6 py-4 border-b border-rule-2 flex items-center justify-between" {
                    h2 class="text-lg font-bold text-ink" { "Cancel practice" }
                    button type="button"
                           class="text-muted hover:text-ink-2 text-xl leading-none"
                           "aria-label"="Close"
                           onclick=(CANCEL_CLOSE_JS) {
                        span "aria-hidden"="true" { "\u{00d7}" }
                    }
                }
                div class="px-6 py-4" {
                    p class="text-sm text-ink-2" {
                        "Rowers have already been notified about this practice. Would you like to send cancellation emails?"
                    }
                }
                div class="px-6 py-4 border-t border-rule-2 flex justify-end gap-3" {
                    form method="post" action=(silent_url)
                         hx-post=(silent_url)
                         hx-target="#content"
                         onclick=(CANCEL_CLOSE_JS) {
                        button type="submit"
                               class="text-sm font-semibold text-ink-3 hover:text-ink px-3 py-2" {
                            "Cancel without notifying"
                        }
                    }
                    form method="post" action=(notify_url)
                         hx-post=(notify_url)
                         hx-target="body"
                         hx-swap="beforeend"
                         onclick=(CANCEL_CLOSE_JS) {
                        button type="submit"
                               class="bg-red-600 hover:bg-red-700 text-white font-semibold px-4 py-2 rounded shadow-soft transition text-sm" {
                            "Cancel and notify rowers"
                        }
                    }
                }
            }
        }
        script { (maud::PreEscaped("trapFocus(document.getElementById('cancel-modal'));")) }
    }
}
