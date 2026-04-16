//! Public landing page for unauthenticated visitors.

use maud::{html, Markup, DOCTYPE};

pub(crate) fn landing_page() -> Markup {
    let source_url = std::env::var("SOURCE_URL")
        .unwrap_or_else(|_| "https://github.com/TODO/oarful".to_string());

    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Oarful — Lineup Generator for Rowing Clubs" }
                link rel="stylesheet" href="/tailwind.css";
            }
            body class="bg-slate-50 text-slate-900 min-h-screen flex flex-col" {
                // Nav bar
                nav class="px-6 py-4 flex items-center justify-between max-w-5xl mx-auto w-full" {
                    span class="text-xl font-bold text-slate-800" { "Oarful" }
                    div class="flex items-center gap-3" {
                        a href="/login"
                          class="text-sm text-slate-600 hover:text-slate-800 transition" {
                            "Sign in"
                        }
                        a href="/signup"
                          class="text-sm font-semibold bg-slate-800 text-white px-4 py-2 rounded hover:bg-slate-900 transition" {
                            "Get started"
                        }
                    }
                }

                // Hero
                main class="flex-grow flex flex-col items-center justify-center px-6 pb-16" {
                    div class="max-w-2xl text-center" {
                        h1 class="text-4xl sm:text-5xl font-bold text-slate-800 leading-tight" {
                            "Practice lineups,"
                            br;
                            "without the spreadsheet."
                        }
                        p class="mt-4 text-lg text-slate-600 max-w-lg mx-auto" {
                            "Oarful is a lineup generator for rowing clubs. "
                            "Set availability, define constraints, and let the solver "
                            "do the rest."
                        }
                        div class="mt-8 flex flex-col sm:flex-row items-center justify-center gap-3" {
                            a href="/signup"
                              class="bg-slate-800 text-white font-semibold px-6 py-3 rounded-lg hover:bg-slate-900 transition text-sm" {
                                "Get started free"
                            }
                            form method="post" action="/demo" {
                                button type="submit"
                                       class="border border-slate-300 text-slate-600 hover:text-slate-800 hover:border-slate-400 font-medium px-6 py-3 rounded-lg transition text-sm" {
                                    "Try the demo"
                                }
                            }
                        }
                    }

                    // Feature highlights
                    div class="mt-16 max-w-3xl w-full grid grid-cols-1 sm:grid-cols-3 gap-6" {
                        (feature_card(
                            "Constraint solver",
                            "Seat affinities, pair preferences, weight classes — the solver handles it all.",
                        ))
                        (feature_card(
                            "Availability tracking",
                            "Rowers mark their own availability. Coaches see who's in at a glance.",
                        ))
                        (feature_card(
                            "Multi-team support",
                            "Run multiple squads under one club. Cross-team coordination built in.",
                        ))
                    }
                }

                // Footer
                footer class="text-center text-xs text-slate-400 py-4" {
                    "30-day free trial · No credit card required"
                    span class="mx-2" { "·" }
                    a href=(source_url) target="_blank"
                      class="hover:text-slate-600 transition" {
                        "Source (AGPL-3.0)"
                    }
                }
            }
        }
    }
}

fn feature_card(title: &str, desc: &str) -> Markup {
    html! {
        div class="bg-white rounded-lg shadow-sm border border-slate-200 p-5" {
            h3 class="font-semibold text-slate-800 mb-1" { (title) }
            p class="text-sm text-slate-600" { (desc) }
        }
    }
}
