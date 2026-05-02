//! Modal shown after sending emails (reminders or lineups).
//!
//! Replaces the old green-banner `send_result()` with a structured
//! modal listing recipients by name, or showing a billing gate message.

use maud::{html, Markup};

/// A recipient in the send result.
pub(crate) struct SendResultRecipient {
    pub(crate) name: String,
    pub(crate) status: SendStatus,
}

/// Whether a recipient was emailed, skipped, or blocked.
pub(crate) enum SendStatus {
    Sent,
    Failed,
}

const CLOSE_JS: &str = "releaseFocus(); document.getElementById('send-result-modal').remove(); \
                         document.getElementById('send-result-modal-backdrop').remove()";

/// Modal shown when email sending is blocked by billing status.
/// When `stripe_enabled` is true, shows an "Upgrade" button linking to checkout.
pub(crate) fn send_result_billing_gate(message: &str, stripe_enabled: bool) -> Markup {
    result_modal_shell(
        "Email unavailable",
        html! {
            div class="px-6 py-4" {
                div class="flex items-center gap-3 mb-3" {
                    span class="text-2xl" "aria-hidden"="true" { "\u{1f512}" }
                    p class="text-sm text-ink-2" { (message) }
                }
                @if stripe_enabled {
                    form method="post" action="/billing/checkout" class="mt-4" {
                        button type="submit"
                               class="bg-ink hover:bg-ink-2 text-paper font-semibold px-4 py-2 rounded shadow-soft transition text-sm" {
                            "Upgrade"
                        }
                    }
                }
            }
        },
    )
}

/// Modal shown after a send attempt with per-recipient results.
pub(crate) fn send_result_modal(title: &str, recipients: &[SendResultRecipient]) -> Markup {
    let sent_count = recipients
        .iter()
        .filter(|r| matches!(r.status, SendStatus::Sent))
        .count();
    let failed_count = recipients
        .iter()
        .filter(|r| matches!(r.status, SendStatus::Failed))
        .count();

    result_modal_shell(
        title,
        html! {
            div class="px-6 py-4" {
                @if recipients.is_empty() {
                    p class="text-sm text-ink-3 italic" {
                        "No recipients to email."
                    }
                } @else {
                    p class="text-sm text-ink-2 mb-3" {
                        @if sent_count > 0 && failed_count == 0 {
                            "Sent to " strong { (sent_count) } " recipient(s)."
                        } @else if sent_count > 0 {
                            "Sent to " strong { (sent_count) }
                            ", failed for " strong { (failed_count) } "."
                        } @else {
                            "No emails sent."
                        }
                    }
                    div class="space-y-1 max-h-60 overflow-y-auto" {
                        @for r in recipients {
                            div class="flex items-center justify-between text-sm py-1" {
                                span class="text-ink" { (r.name) }
                                @match r.status {
                                    SendStatus::Sent => {
                                        span class="text-xs text-emerald-600 font-medium" { "\u{2713} Sent" }
                                    }
                                    SendStatus::Failed => {
                                        span class="text-xs text-red-600 font-medium" { "\u{2717} Failed" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
    )
}

fn result_modal_shell(title: &str, body: Markup) -> Markup {
    html! {
        div id="send-result-modal-backdrop"
            class="fixed inset-0 bg-black/40 z-40"
            onclick=(CLOSE_JS) {}
        div id="send-result-modal"
            role="dialog"
            "aria-modal"="true"
            "aria-label"="Send results"
            class="fixed inset-0 z-50 flex items-start justify-center pt-12 px-4 pointer-events-none" {
            div class="bg-paper rounded-lg shadow-xl w-full max-w-lg max-h-[80vh] overflow-y-auto pointer-events-auto" {
                div class="sticky top-0 bg-paper border-b border-rule-2 px-6 py-4 flex items-center justify-between" {
                    h2 class="text-lg font-bold text-ink" { (title) }
                    button type="button"
                           class="text-muted hover:text-ink-2 text-xl leading-none"
                           "aria-label"="Close"
                           onclick=(CLOSE_JS) {
                        span "aria-hidden"="true" { "\u{00d7}" }
                    }
                }
                (body)
                div class="sticky bottom-0 bg-paper border-t border-rule-2 px-6 py-3 flex justify-end" {
                    button type="button"
                           class="bg-ink hover:bg-ink-2 text-paper font-semibold px-4 py-2 rounded shadow-soft transition text-sm"
                           onclick=(CLOSE_JS) {
                        "Close"
                    }
                }
            }
        }
        script { (maud::PreEscaped("trapFocus(document.getElementById('send-result-modal'));")) }
    }
}
