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
                link rel="stylesheet" href="/tailwind.css";
            }
            body class="bg-paper text-ink min-h-screen flex items-center justify-center" {
                div class="w-full max-w-sm" {
                    div class="text-center mb-8" {
                        h1 class="text-2xl font-bold text-ink" { "Oarful" }
                        p class="text-sm text-ink-3 mt-1" { "Sign in to your club" }
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
        p class="mt-8 text-center text-xs text-muted" {
            a href=(url) target="_blank"
              class="hover:text-ink-2 transition" {
                "Source code (AGPL-3.0)"
            }
        }
    }
}

fn error_banner(msg: &str) -> Markup {
    html! {
        div class="mb-4 bg-bad/10 border-l-4 border-bad px-4 py-3 rounded text-sm text-ink" {
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
    login_email_step(error, None, false, None)
}

pub(crate) fn login_email_step(
    error: Option<&str>,
    prefill_email: Option<&str>,
    has_demo_cookie: bool,
    success: Option<&str>,
) -> Markup {
    auth_shell(
        "Login",
        html! {
            @if let Some(msg) = success {
                (success_banner(msg))
            }
            @if let Some(msg) = error {
                (error_banner(msg))
            }

            form method="post" action="/login/email"
                 class="bg-paper rounded-lg shadow-soft p-6 space-y-4" {
                div {
                    label for="email" class="block text-sm font-semibold text-ink-2 mb-1" { "Email" }
                    input id="email" name="email" type="email" required autofocus
                          autocomplete="email"
                          value=[prefill_email]
                          class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                }
                button type="submit"
                       class="w-full btn-warm-ink py-2" {
                    "Continue"
                }
            }

            // Demo section
            div class="mt-6 text-center" {
                div class="text-xs text-muted mb-2" { "or" }
                @if has_demo_cookie {
                    form method="post" action="/demo/resume" {
                        button type="submit"
                               class="w-full border border-rule hover:border-rule text-ink-2 hover:text-ink font-medium py-2 rounded transition text-sm" {
                            "Resume demo"
                        }
                    }
                } @else {
                    form method="post" action="/demo" {
                        button type="submit"
                               class="w-full border border-rule hover:border-rule text-ink-2 hover:text-ink font-medium py-2 rounded transition text-sm" {
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
                 class="bg-paper rounded-lg shadow-soft p-6 space-y-4" {
                // Show email as read-only context
                div {
                    label class="block text-sm font-semibold text-ink-2 mb-1" { "Email" }
                    div class="flex items-center gap-2" {
                        span class="text-sm text-ink-2" { (email) }
                        a href="/login" class="text-xs text-muted hover:text-ink-2" { "change" }
                    }
                    input type="hidden" name="email" value=(email);
                }
                div {
                    label for="password" class="block text-sm font-semibold text-ink-2 mb-1" { "Password" }
                    input id="password" name="password" type="password" required autofocus
                          autocomplete="current-password"
                          class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                    div class="mt-1 text-right" {
                        a href={"/forgot-password?email=" (email)}
                          class="text-xs text-ink-3 hover:text-ink-2" {
                            "Forgot password?"
                        }
                    }
                }
                button type="submit"
                       class="w-full btn-warm-ink py-2" {
                    "Sign in"
                }
            }

            @if show_magic_link {
                form method="post" action="/login/magic"
                     class="mt-3" {
                    input type="hidden" name="email" value=(email);
                    div class="text-center" {
                        span class="text-xs text-muted" { "or" }
                    }
                    button type="submit"
                           class="w-full mt-2 border border-rule hover:border-rule text-ink-2 hover:text-ink font-medium py-2 rounded transition text-sm" {
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
            div class="bg-paper rounded-lg shadow-soft p-6 space-y-4" {
                p class="text-sm text-ink-2 mb-2" {
                    "Your email is associated with multiple clubs. Choose one:"
                }
                @for (tenant_id, name, role) in clubs {
                    form method="post" action="/login/pick" {
                        input type="hidden" name="email" value=(email);
                        input type="hidden" name="password" value=(password);
                        input type="hidden" name="tenant_id" value=(tenant_id);
                        button type="submit"
                               class="w-full text-left border border-rule-2 hover:border-rule rounded-lg px-4 py-3 transition flex items-center justify-between" {
                            div {
                                div class="font-semibold text-ink" { (name) }
                                @if let Some(r) = role {
                                    div class="text-xs text-ink-3" { (r) }
                                }
                            }
                            span class="text-muted text-sm" "aria-hidden"="true" { "\u{2192}" }
                        }
                    }
                }
                a href="/login" class="inline-block mt-2 text-sm text-ink-3 hover:text-ink-2" {
                    "Back"
                }
            }
        },
    )
}

/// Forgot-password page: email input + "Send reset link" button.
pub(crate) fn forgot_password_page(prefill_email: Option<&str>) -> Markup {
    auth_shell(
        "Forgot password",
        html! {
            form method="post" action="/forgot-password"
                 class="bg-paper rounded-lg shadow-soft p-6 space-y-4" {
                div class="text-sm text-ink-2 mb-2" {
                    "Enter your email and we'll send you a link to reset your password."
                }
                div {
                    label for="email" class="block text-sm font-semibold text-ink-2 mb-1" { "Email" }
                    input id="email" name="email" type="email" required autofocus
                          autocomplete="email"
                          value=[prefill_email]
                          class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                }
                button type="submit"
                       class="w-full btn-warm-ink py-2" {
                    "Send reset link"
                }
            }
            div class="mt-4 text-center" {
                a href="/login" class="text-sm text-ink-3 hover:text-ink-2" {
                    "Back to sign in"
                }
            }
        },
    )
}

/// Confirmation after sending a password-reset email.
pub(crate) fn forgot_password_sent(email: &str) -> Markup {
    auth_shell(
        "Check your email",
        html! {
            (success_banner("Password reset link sent! Check your inbox."))
            div class="bg-paper rounded-lg shadow-soft p-6 text-center space-y-3" {
                p class="text-sm text-ink-2" {
                    "We sent a password reset link to "
                    strong { (email) }
                    ". Click the link to set a new password."
                }
                p class="text-xs text-muted" {
                    "The link expires in 1 hour."
                }
                a href="/login" class="inline-block mt-2 text-sm text-ink-3 hover:text-ink-2" {
                    "Back to sign in"
                }
            }
        },
    )
}

/// Password-reset form (user is authenticated via magic link).
pub(crate) fn reset_password_form(error: Option<&str>) -> Markup {
    auth_shell(
        "Reset password",
        html! {
            @if let Some(msg) = error {
                (error_banner(msg))
            }
            form method="post" action="/reset-password"
                 class="bg-paper rounded-lg shadow-soft p-6 space-y-4" {
                div class="header text-lg font-bold text-ink mb-2" { "Set a new password" }
                div {
                    label for="password" class="block text-sm font-semibold text-ink-2 mb-1" { "New password" }
                    input id="password" name="password" type="password" required autofocus
                          autocomplete="new-password"
                          minlength="8"
                          class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                }
                div {
                    label for="password_confirm" class="block text-sm font-semibold text-ink-2 mb-1" { "Confirm password" }
                    input id="password_confirm" name="password_confirm" type="password" required
                          autocomplete="new-password"
                          minlength="8"
                          class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                }
                button type="submit"
                       class="w-full btn-warm-ink py-2" {
                    "Update password"
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
            div class="bg-paper rounded-lg shadow-soft p-6 text-center space-y-3" {
                p class="text-sm text-ink-2" {
                    "We sent a sign-in link to "
                    strong { (email) }
                    ". Click the link to sign in."
                }
                p class="text-xs text-muted" {
                    "The link expires in 24 hours."
                }
                a href="/login" class="inline-block mt-2 text-sm text-ink-3 hover:text-ink-2" {
                    "Back to sign in"
                }
            }
        },
    )
}
