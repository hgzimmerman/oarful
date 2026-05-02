//! Signup form for new club registration.

use maud::{html, Markup, DOCTYPE};

pub(crate) fn signup_page(error: Option<&str>, prefill: &SignupPrefill) -> Markup {
    let source_url = std::env::var("SOURCE_URL")
        .unwrap_or_else(|_| "https://github.com/TODO/oarful".to_string());

    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Sign up · Oarful" }
                link rel="stylesheet" href="/tailwind.css";
            }
            body class="bg-paper text-ink min-h-screen flex items-center justify-center" {
                div class="w-full max-w-sm" {
                    div class="text-center mb-8" {
                        h1 class="text-2xl font-bold text-ink" { "Oarful" }
                        p class="text-sm text-ink-3 mt-1" { "Create your club" }
                    }

                    @if let Some(msg) = error {
                        div class="mb-4 bg-bad/10 border-l-4 border-bad px-4 py-3 rounded text-sm text-ink" {
                            (msg)
                        }
                    }

                    form method="post" action="/signup"
                         class="bg-paper rounded-lg shadow-soft p-6 space-y-4" {
                        div {
                            label for="club_name" class="block text-sm font-semibold text-ink-2 mb-1" { "Club name" }
                            input id="club_name" name="club_name" type="text" required autofocus
                                  value=(prefill.club_name)
                                  placeholder="Your club or organization"
                                  class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                        }
                        div {
                            label for="name" class="block text-sm font-semibold text-ink-2 mb-1" { "Your name" }
                            input id="name" name="name" type="text" required
                                  value=(prefill.name)
                                  class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                        }
                        div {
                            label for="email" class="block text-sm font-semibold text-ink-2 mb-1" { "Email" }
                            input id="email" name="email" type="email" required
                                  value=(prefill.email)
                                  class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                        }
                        div {
                            label for="password" class="block text-sm font-semibold text-ink-2 mb-1" { "Password" }
                            input id="password" name="password" type="password" required
                                  minlength="8"
                                  class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                        }
                        div {
                            label for="password_confirm" class="block text-sm font-semibold text-ink-2 mb-1" { "Confirm password" }
                            input id="password_confirm" name="password_confirm" type="password" required
                                  minlength="8"
                                  class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                        }
                        button type="submit"
                               class="w-full bg-ink hover:bg-ink-2 text-paper font-semibold py-2 rounded shadow-soft transition" {
                            "Create club"
                        }
                    }

                    p class="mt-4 text-center text-sm text-ink-3" {
                        "Already have an account? "
                        a href="/login" class="text-ink-2 hover:text-ink font-medium" { "Sign in" }
                    }

                    p class="mt-6 text-center text-xs text-muted" {
                        a href=(source_url) target="_blank"
                          class="hover:text-ink-2 transition" {
                            "Source code (AGPL-3.0)"
                        }
                    }
                }
            }
        }
    }
}

/// Rendered when `SIGNUP_DISABLED=1` — tells visitors signup is closed.
pub(crate) fn signup_closed_page() -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Signup closed · Oarful" }
                link rel="stylesheet" href="/tailwind.css";
            }
            body class="bg-paper text-ink min-h-screen flex items-center justify-center" {
                div class="w-full max-w-sm text-center" {
                    h1 class="text-2xl font-bold text-ink" { "Oarful" }
                    p class="mt-4 text-ink-2" {
                        "Signup is currently closed."
                    }
                    p class="mt-2 text-sm text-ink-3" {
                        "If you already have an account, you can "
                        a href="/login" class="text-ink-2 hover:text-ink font-medium" { "sign in" }
                        "."
                    }
                    p class="mt-2 text-sm text-ink-3" {
                        "Or "
                        a href="/" class="text-ink-2 hover:text-ink font-medium" { "try the demo" }
                        " to see how it works."
                    }
                }
            }
        }
    }
}

/// Prefill values for re-rendering the form after validation errors.
#[derive(Default)]
pub(crate) struct SignupPrefill {
    pub club_name: String,
    pub name: String,
    pub email: String,
}
