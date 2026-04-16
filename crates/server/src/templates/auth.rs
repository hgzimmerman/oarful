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
                title { (title) " · Oarful" }
                script src="/tailwindcss.js" {}
            }
            body class="bg-slate-50 text-slate-900 min-h-screen flex items-center justify-center" {
                div class="w-full max-w-sm" {
                    div class="text-center mb-8" {
                        h1 class="text-2xl font-bold text-slate-800" { "Oarful" }
                        p class="text-sm text-slate-500 mt-1" { "Sign in to your club" }
                    }
                    (body)
                    (source_link())
                }
            }
        }
    }
}

fn source_link() -> Markup {
    let url = std::env::var("SOURCE_URL")
        .unwrap_or_else(|_| "https://github.com/TODO/oarful".to_string());
    html! {
        p class="mt-8 text-center text-xs text-slate-400" {
            a href=(url) target="_blank"
              class="hover:text-slate-600 transition" {
                "Source code (AGPL-3.0)"
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
    login_email_step(error, None, false)
}

pub(crate) fn login_email_step(
    error: Option<&str>,
    prefill_email: Option<&str>,
    has_demo_cookie: bool,
) -> Markup {
    auth_shell(
        "Login",
        html! {
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

            // Demo section
            div class="mt-6 text-center" {
                div class="text-xs text-slate-400 mb-2" { "or" }
                @if has_demo_cookie {
                    form method="post" action="/demo/resume" {
                        button type="submit"
                               class="w-full border border-slate-300 hover:border-slate-400 text-slate-600 hover:text-slate-800 font-medium py-2 rounded transition text-sm" {
                            "Resume demo"
                        }
                    }
                } @else {
                    form method="post" action="/demo" {
                        button type="submit"
                               class="w-full border border-slate-300 hover:border-slate-400 text-slate-600 hover:text-slate-800 font-medium py-2 rounded transition text-sm" {
                            "Try demo"
                        }
                    }
                }
            }
        },
    )
}

/// Step 2: password form + optional magic link button.
/// `show_magic_link` is true when the `known_user` cookie is present.
pub(crate) fn login_password_step(
    email: &str,
    error: Option<&str>,
    show_magic_link: bool,
) -> Markup {
    auth_shell(
        "Login",
        html! {
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
        },
    )
}

/// Step 3 (optional): club picker when the user's email matches
/// multiple tenants. Each button posts to `/login/pick` with the
/// chosen tenant_id.
pub(crate) fn login_club_picker(
    email: &str,
    password: &str,
    clubs: &[(i32, String, Option<String>)], // (tenant_id, name, role)
) -> Markup {
    auth_shell(
        "Choose your club",
        html! {
            div class="bg-white rounded-lg shadow p-6 space-y-4" {
                p class="text-sm text-slate-600 mb-2" {
                    "Your email is associated with multiple clubs. Choose one:"
                }
                @for (tenant_id, name, role) in clubs {
                    form method="post" action="/login/pick" {
                        input type="hidden" name="email" value=(email);
                        input type="hidden" name="password" value=(password);
                        input type="hidden" name="tenant_id" value=(tenant_id);
                        button type="submit"
                               class="w-full text-left border border-slate-200 hover:border-slate-400 rounded-lg px-4 py-3 transition flex items-center justify-between" {
                            div {
                                div class="font-semibold text-slate-800" { (name) }
                                @if let Some(r) = role {
                                    div class="text-xs text-slate-500" { (r) }
                                }
                            }
                            span class="text-slate-400 text-sm" { "\u{2192}" }
                        }
                    }
                }
                a href="/login" class="inline-block mt-2 text-sm text-slate-500 hover:text-slate-700" {
                    "Back"
                }
            }
        },
    )
}

/// Confirmation after sending a magic link login email.
pub(crate) fn login_magic_sent(email: &str) -> Markup {
    auth_shell(
        "Check your email",
        html! {
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
        },
    )
}
