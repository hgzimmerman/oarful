//! Standalone unsubscribe confirmation pages (no app layout — user is
//! not logged in).

use maud::{html, Markup, DOCTYPE};

use crate::unsubscribe::EmailType;

fn standalone_page(title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) }
                style {
                    "body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; \
                     margin: 0; padding: 0; background: #f1f5f9; color: #1e293b; \
                     display: flex; justify-content: center; align-items: center; min-height: 100vh; } \
                     .card { background: #ffffff; border-radius: 8px; padding: 32px; max-width: 480px; \
                     margin: 24px; box-shadow: 0 1px 3px rgba(0,0,0,0.1); text-align: center; } \
                     h1 { font-size: 20px; margin: 0 0 12px; } \
                     p { font-size: 14px; color: #475569; line-height: 1.5; margin: 8px 0; } \
                     .muted { font-size: 12px; color: #94a3b8; margin-top: 20px; }"
                }
            }
            body {
                div class="card" {
                    (body)
                }
            }
        }
    }
}

pub(crate) fn success_page(club_name: &str, email_type: EmailType) -> Markup {
    let type_label = match email_type {
        EmailType::Reminders => "availability reminder",
        EmailType::Lineups => "lineup notification",
        EmailType::StaleAlerts => "lineup change alert",
        EmailType::All => "all",
    };
    standalone_page(
        "Unsubscribed",
        html! {
            h1 { "Unsubscribed" }
            p {
                "You have been unsubscribed from " (type_label)
                " emails for " strong { (club_name) } "."
            }
            p class="muted" {
                "To re-subscribe, log in and visit your email preferences."
            }
        },
    )
}

pub(crate) fn error_page(message: &str) -> Markup {
    standalone_page(
        "Unsubscribe",
        html! {
            h1 { "Unsubscribe" }
            p { (message) }
        },
    )
}
