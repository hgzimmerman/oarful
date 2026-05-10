//! Reusable HTMX confirmation modal.
//!
//! Replaces native `hx-confirm` / `window.confirm()` with a styled
//! modal that matches the rest of the UI. Usage:
//!
//! 1. Trigger button: `hx-get="/confirm-endpoint" hx-target="body" hx-swap="beforeend"`
//! 2. Endpoint returns `confirm_modal(...)` markup
//! 3. User clicks Confirm → the inner form performs the actual action

use maud::{html, Markup};

const CLOSE_JS: &str = "dismissModal('confirm-modal', 'confirm-modal-backdrop')";

/// Render a confirmation modal.
///
/// - `title`: modal header (e.g. "Archive team")
/// - `message`: body text explaining the consequence
/// - `confirm_label`: text on the confirm button (e.g. "Archive", "Delete", "Deactivate")
/// - `action_markup`: the inner form/button that performs the actual action.
///   Typically a `<form hx-post="..." hx-disabled-elt="find button" hx-target="...">` with a submit button.
///   The modal provides the shell; this slot provides the action.
pub(crate) fn confirm_modal(title: &str, message: &str, action_markup: Markup) -> Markup {
    html! {
        div id="confirm-modal-backdrop"
            class="fixed inset-0 bg-black/40 z-40 modal-backdrop"
            onclick=(CLOSE_JS) {}
        div id="confirm-modal"
            role="dialog"
            "aria-modal"="true"
            "aria-labelledby"="confirm-modal-title"
            class="fixed inset-0 z-50 flex items-start justify-center pt-24 px-4 pointer-events-none" {
            div class="bg-paper rounded-lg shadow-xl w-full max-w-md pointer-events-auto modal-card" {
                div class="px-6 py-4 border-b border-rule-2 flex items-center justify-between" {
                    h2 id="confirm-modal-title" class="text-lg font-bold text-ink" { (title) }
                    button type="button"
                           class="text-muted hover:text-ink-2 text-xl leading-none"
                           "aria-label"="Close"
                           onclick=(CLOSE_JS) {
                        span "aria-hidden"="true" { "\u{00d7}" }
                    }
                }
                div class="px-6 py-4" {
                    p class="text-sm text-ink-2" { (message) }
                }
                div class="px-6 py-4 border-t border-rule-2 flex justify-end gap-3" {
                    button type="button"
                           class="px-4 py-2 text-sm font-medium text-ink-2 hover:text-ink transition"
                           onclick=(CLOSE_JS) {
                        "Cancel"
                    }
                    (action_markup)
                }
            }
        }
        script { (maud::PreEscaped("trapFocus(document.getElementById('confirm-modal'));")) }
    }
}
