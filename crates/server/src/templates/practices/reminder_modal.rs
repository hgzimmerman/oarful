//! Reminder preview modal template.

use chrono::NaiveDate;
use maud::{html, Markup, PreEscaped};

/// Recipient info for the reminder preview modal.
pub(crate) struct ReminderRecipientPreview {
    pub(crate) name: String,
    pub(crate) dates: Vec<NaiveDate>,
}

const CLOSE_JS: &str = "document.getElementById('reminder-modal').remove(); document.getElementById('reminder-modal-backdrop').remove()";

pub(crate) fn reminder_preview_modal(
    recipients: &[ReminderRecipientPreview],
    practice_ids: &[i32],
) -> Markup {
    let unique_count = recipients.len();
    let practice_count: usize = {
        let mut dates: Vec<&NaiveDate> = recipients.iter().flat_map(|r| r.dates.iter()).collect();
        dates.sort();
        dates.dedup();
        dates.len()
    };

    html! {
        div id="reminder-modal-backdrop"
            class="fixed inset-0 bg-black/40 z-40"
            onclick=(CLOSE_JS) {}
        div id="reminder-modal"
            class="fixed inset-0 z-50 flex items-start justify-center pt-12 px-4 pointer-events-none" {
            div class="bg-white rounded-lg shadow-xl w-full max-w-lg max-h-[80vh] overflow-y-auto pointer-events-auto" {
                div class="sticky top-0 bg-white border-b border-slate-200 px-6 py-4 flex items-center justify-between" {
                    h2 class="text-lg font-bold text-slate-800" { "Send reminders" }
                    button type="button"
                           class="text-slate-400 hover:text-slate-600 text-xl leading-none"
                           onclick=(CLOSE_JS) {
                        "\u{00d7}"
                    }
                }
                div class="px-6 py-4" {
                    @if recipients.is_empty() {
                        p class="text-sm text-slate-500 italic" {
                            "No reminders to send — everyone has responded or reminders were already sent today."
                        }
                    } @else {
                        p class="text-sm text-slate-600 mb-3" {
                            "Will email " strong { (unique_count) }
                            " rower(s) about "
                            strong { (practice_count) }
                            " practice(s):"
                        }
                        div class="space-y-1 mb-4 max-h-60 overflow-y-auto" {
                            @for r in recipients {
                                div class="flex items-center justify-between text-sm py-1" {
                                    span class="text-slate-800" { (r.name) }
                                    span class="text-xs text-slate-400" {
                                        @for (i, d) in r.dates.iter().enumerate() {
                                            @if i > 0 { ", " }
                                            (d.format("%b %-d"))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                @if !recipients.is_empty() {
                    div class="sticky bottom-0 bg-white border-t border-slate-200 px-6 py-4 flex justify-end" {
                        form method="post" action="/practices/send-reminders"
                             hx-post="/practices/send-reminders"
                             hx-target="body"
                             hx-swap="beforeend"
                             onclick=(PreEscaped(CLOSE_JS)) {
                            @for id in practice_ids {
                                input type="hidden" name="practice_ids" value=(id);
                            }
                            button type="submit"
                                   class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                                "Send " (unique_count) " reminder(s)"
                            }
                        }
                    }
                }
            }
        }
    }
}
