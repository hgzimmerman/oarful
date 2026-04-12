//! Auth templates: two-step login flow.

use maud::{html, Markup, DOCTYPE};

/// Shared page wrapper for auth pages (no navbar).
fn auth_shell(title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " · Lineup Generator" }
                script src="https://cdn.tailwindcss.com" {}
            }
            body class="bg-slate-50 text-slate-900 min-h-screen flex items-center justify-center" {
                div class="w-full max-w-sm" {
                    div class="text-center mb-8" {
                        h1 class="text-2xl font-bold text-slate-800" { "Lineup Generator" }
                        p class="text-sm text-slate-500 mt-1" { "Sign in to your club" }
                    }
                    (body)
                }
            }
        }
    }
}

fn error_banner(msg: &str) -> Markup {
    html! {
        div class="mb-4 bg-red-50 border-l-4 border-red-500 px-4 py-3 rounded text-sm text-red-900" {
            (msg)
        }
    }
}

fn success_banner(msg: &str) -> Markup {
    html! {
        div class="mb-4 bg-emerald-50 border-l-4 border-emerald-500 px-4 py-3 rounded text-sm text-emerald-900" {
            (msg)
        }
    }
}

/// Step 1: email-only form. `prefill_email` comes from the long-lived
/// `known_user` cookie.
pub(crate) fn login_page(error: Option<&str>) -> Markup {
    login_email_step(error, None)
}

pub(crate) fn login_email_step(error: Option<&str>, prefill_email: Option<&str>) -> Markup {
    auth_shell("Login", html! {
        @if let Some(msg) = error {
            (error_banner(msg))
        }

        form method="post" action="/login/email"
             class="bg-white rounded-lg shadow p-6 space-y-4" {
            div {
                label for="email" class="block text-sm font-semibold text-slate-700 mb-1" { "Email" }
                input id="email" name="email" type="email" required autofocus
                      value=[prefill_email]
                      class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
            }
            button type="submit"
                   class="w-full bg-slate-800 hover:bg-slate-900 text-white font-semibold py-2 rounded shadow transition" {
                "Continue"
            }
        }
    })
}

/// Step 2: password form + optional magic link button.
/// `show_magic_link` is true when the `known_user` cookie is present.
pub(crate) fn login_password_step(
    email: &str,
    error: Option<&str>,
    show_magic_link: bool,
) -> Markup {
    auth_shell("Login", html! {
        @if let Some(msg) = error {
            (error_banner(msg))
        }

        form method="post" action="/login"
             class="bg-white rounded-lg shadow p-6 space-y-4" {
            // Show email as read-only context
            div {
                label class="block text-sm font-semibold text-slate-700 mb-1" { "Email" }
                div class="flex items-center gap-2" {
                    span class="text-sm text-slate-600" { (email) }
                    a href="/login" class="text-xs text-slate-400 hover:text-slate-600" { "change" }
                }
                input type="hidden" name="email" value=(email);
            }
            div {
                label for="password" class="block text-sm font-semibold text-slate-700 mb-1" { "Password" }
                input id="password" name="password" type="password" required autofocus
                      class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
            }
            button type="submit"
                   class="w-full bg-slate-800 hover:bg-slate-900 text-white font-semibold py-2 rounded shadow transition" {
                "Sign in"
            }
        }

        @if show_magic_link {
            form method="post" action="/login/magic"
                 class="mt-3" {
                input type="hidden" name="email" value=(email);
                div class="text-center" {
                    span class="text-xs text-slate-400" { "or" }
                }
                button type="submit"
                       class="w-full mt-2 border border-slate-300 hover:border-slate-400 text-slate-600 hover:text-slate-800 font-medium py-2 rounded transition text-sm" {
                    "Email me a sign-in link"
                }
            }
        }
    })
}

/// Confirmation after sending a magic link login email.
pub(crate) fn login_magic_sent(email: &str) -> Markup {
    auth_shell("Check your email", html! {
        (success_banner("Sign-in link sent! Check your inbox."))
        div class="bg-white rounded-lg shadow p-6 text-center space-y-3" {
            p class="text-sm text-slate-600" {
                "We sent a sign-in link to "
                strong { (email) }
                ". Click the link to sign in."
            }
            p class="text-xs text-slate-400" {
                "The link expires in 24 hours."
            }
            a href="/login" class="inline-block mt-2 text-sm text-slate-500 hover:text-slate-700" {
                "Back to sign in"
            }
        }
    })
}
