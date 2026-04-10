//! Auth templates: login page, invite page.

use maud::{html, Markup, DOCTYPE};

/// Standalone login page (no navbar — user isn't authenticated yet).
pub(crate) fn login_page(error: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Login · Lineup Generator" }
                script src="https://cdn.tailwindcss.com" {}
            }
            body class="bg-slate-50 text-slate-900 min-h-screen flex items-center justify-center" {
                div class="w-full max-w-sm" {
                    div class="text-center mb-8" {
                        h1 class="text-2xl font-bold text-slate-800" { "Lineup Generator" }
                        p class="text-sm text-slate-500 mt-1" { "Sign in to your club" }
                    }

                    @if let Some(msg) = error {
                        div class="mb-4 bg-red-50 border-l-4 border-red-500 px-4 py-3 rounded text-sm text-red-900" {
                            (msg)
                        }
                    }

                    form method="post" action="/login"
                         class="bg-white rounded-lg shadow p-6 space-y-4" {
                        div {
                            label for="email" class="block text-sm font-semibold text-slate-700 mb-1" { "Email" }
                            input id="email" name="email" type="email" required autofocus
                                  class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                        }
                        div {
                            label for="password" class="block text-sm font-semibold text-slate-700 mb-1" { "Password" }
                            input id="password" name="password" type="password" required
                                  class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                        }
                        button type="submit"
                               class="w-full bg-slate-800 hover:bg-slate-900 text-white font-semibold py-2 rounded shadow transition" {
                            "Sign in"
                        }
                    }
                }
            }
        }
    }
}
