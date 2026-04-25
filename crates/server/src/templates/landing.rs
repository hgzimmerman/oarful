//! Public landing page for unauthenticated visitors.

use maud::{html, Markup, DOCTYPE};

pub(crate) fn landing_page(signup_disabled: bool) -> Markup {
    let source_url = std::env::var("SOURCE_URL")
        .unwrap_or_else(|_| "https://github.com/TODO/oarful".to_string());

    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Oarful 🚣 Lineup Generator for Rowing Clubs" }
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
                        @if !signup_disabled {
                            a href="/signup"
                              class="text-sm font-semibold bg-slate-800 text-white px-4 py-2 rounded hover:bg-slate-900 transition" {
                                "Get started"
                            }
                        }
                    }
                }

                // Hero
                main class="flex-grow flex flex-col items-center px-6 pt-16 sm:pt-24 pb-16" {
                    div class="max-w-2xl text-center" {
                        h1 class="text-4xl sm:text-5xl font-bold text-slate-800 leading-tight" {
                            "Rowing lineup tool"
                        }
                        p class="mt-4 text-lg text-slate-600 max-w-lg mx-auto" {
                            "Collect availability, generate lineups, "
                            "edit them by hand, send them out."
                        }
                        div class="mt-8 flex flex-col sm:flex-row items-start justify-center gap-8" {
                            @if !signup_disabled {
                                div class="text-center" {
                                    a href="/signup"
                                      class="block bg-slate-800 text-white font-semibold px-8 py-3 rounded-lg hover:bg-slate-900 transition text-sm" {
                                        "Get started free"
                                    }
                                    p class="mt-2 text-xs text-slate-400" { "Free plan, no credit card" }
                                }
                            }
                            div class="text-center" {
                                form method="post" action="/demo" {
                                    button type="submit"
                                           class="w-full border border-slate-300 text-slate-600 hover:text-slate-800 hover:border-slate-400 font-medium px-8 py-3 rounded-lg transition text-sm" {
                                        "Try the demo"
                                    }
                                }
                                p class="mt-2 text-xs text-slate-400" { "Sample data, temporary," br; "expires in 7 days" }
                            }
                        }
                    }

                    // What it does
                    div class="mt-20 max-w-3xl w-full" {
                        h2 class="text-lg font-semibold text-slate-800 mb-4" { "What it does" }
                        div class="grid grid-cols-1 sm:grid-cols-2 gap-x-8 gap-y-6 text-base text-slate-600" {
                            (bullet("Rowers or their coaches set availability and get lineup emails; no more spreadsheets"))
                            (bullet("Lineup generator accounts for seat, side, and pair preferences, height, weight, and skill matching, and weight-class boat eligibility"))
                            (bullet("Lineup editor for manual adjustments, or skip the solver and build lineups manually"))
                            (bullet("Multiple teams per club with boat sharing and double-booking detection"))
                            (bullet("Handle no-shows without redoing lineups from scratch"))
                            (bullet("Boat usage tracking across practices"))
                        }
                    }

                    // Second CTA
                    @if !signup_disabled {
                        div class="mt-16 text-center" {
                            a href="/signup"
                              class="bg-slate-800 text-white font-semibold px-6 py-3 rounded-lg hover:bg-slate-900 transition text-sm" {
                                "Get started free"
                            }
                            p class="mt-2 text-xs text-slate-400" {
                                "Free plan, no credit card"
                            }
                        }
                    }
                }

                // Footer
                footer class="text-center text-xs text-slate-400 py-4" {
                    a href=(source_url) target="_blank"
                      class="hover:text-slate-600 transition" {
                        "Source (AGPL-3.0)"
                    }
                }
            }
        }
    }
}

fn bullet(text: &str) -> Markup {
    html! {
        div class="flex gap-2" {
            span class="text-slate-400 shrink-0" { "·" }
            span { (text) }
        }
    }
}
