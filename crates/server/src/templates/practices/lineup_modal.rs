//! Lineup preview modal template.

use maud::{html, Markup, PreEscaped};

/// Recipient info for the lineup preview modal.
pub(crate) struct LineupRecipientPreview {
    pub(crate) name: String,
}

const CLOSE_JS: &str = "releaseFocus(); document.getElementById('lineup-modal').remove(); document.getElementById('lineup-modal-backdrop').remove()";

pub(crate) fn lineup_preview_modal(
    recipients: &[LineupRecipientPreview],
    date_strs: &[String],
    scope: &str,
) -> Markup {
    let unique_count = recipients.len();
    let date_count = date_strs.len();

    html! {
        div id="lineup-modal-backdrop"
            class="fixed inset-0 bg-black/40 z-40"
            onclick=(CLOSE_JS) {}
        div id="lineup-modal"
            role="dialog"
            "aria-modal"="true"
            "aria-label"="Send lineups"
            class="fixed inset-0 z-50 flex items-start justify-center pt-12 px-4 pointer-events-none" {
            div class="bg-paper rounded-lg shadow-xl w-full max-w-lg max-h-[80vh] overflow-y-auto pointer-events-auto" {
                div class="sticky top-0 bg-paper border-b border-rule-2 px-6 py-4 flex items-center justify-between" {
                    h2 class="text-lg font-bold text-ink" { "Send lineups" }
                    button type="button"
                           class="text-muted hover:text-ink-2 text-xl leading-none"
                           "aria-label"="Close"
                           onclick=(CLOSE_JS) {
                        span "aria-hidden"="true" { "\u{00d7}" }
                    }
                }
                div class="px-6 py-4" {
                    @if date_strs.is_empty() {
                        p class="text-sm text-ink-3 italic" {
                            "No practices selected — check at least one to send lineups."
                        }
                    } @else if recipients.is_empty() {
                        p class="text-sm text-ink-3 italic" {
                            "No recipients — lineups may have already been sent today, or no rowers have accounts with lineup notifications enabled."
                        }
                    } @else {
                        p class="text-sm text-ink-2 mb-3" {
                            "Will email " strong { (unique_count) }
                            " rower(s) about "
                            strong { (date_count) }
                            " lineup(s):"
                        }
                        div class="space-y-1 mb-4 max-h-60 overflow-y-auto" {
                            @for r in recipients {
                                div class="text-sm py-1 text-ink" { (r.name) }
                            }
                        }
                    }
                }
                @if !date_strs.is_empty() && !recipients.is_empty() {
                    div class="sticky bottom-0 bg-paper border-t border-rule-2 px-6 py-4" {
                        form method="post" action="/practices/send-lineups"
                             hx-post="/practices/send-lineups"
                             hx-target="body"
                             hx-swap="beforeend"
                             onclick=(PreEscaped(CLOSE_JS)) {
                            @for d in date_strs {
                                input type="hidden" name="dates" value=(d);
                            }
                            div class="flex items-center gap-4 mb-3" {
                                label class="flex items-center gap-2 text-sm cursor-pointer" {
                                    input type="radio" name="scope" value="placed"
                                          checked[scope == "placed"]
                                          class="text-ink focus:ring-ink-3";
                                    "Placed + bench"
                                }
                                label class="flex items-center gap-2 text-sm cursor-pointer" {
                                    input type="radio" name="scope" value="all"
                                          checked[scope == "all"]
                                          class="text-ink focus:ring-ink-3";
                                    "All (incl. non-respondents)"
                                }
                            }
                            div class="flex justify-end" {
                                button type="submit"
                                       class="bg-ink hover:bg-ink-2 text-paper font-semibold px-4 py-2 rounded shadow-soft transition text-sm" {
                                    "Send " (unique_count) " lineup(s)"
                                }
                            }
                        }
                    }
                }
            }
        }
        script { (maud::PreEscaped("trapFocus(document.getElementById('lineup-modal'));")) }
    }
}
