//! Base page chrome: `<html>` + `<head>` + navbar + `#content` shell.
//!
//! Loads HTMX, Alpine.js, and Tailwind from paths that [`ServeDir`]
//! resolves out of `crates/server/public` in dev. Tailwind comes from
//! the CDN until we add a build pipeline.
//!
//! [`ServeDir`]: tower_http::services::ServeDir

use maud::{html, Markup, DOCTYPE};

use lineup_db::app_user::Role;

pub(crate) fn page(title: &str, content: Markup, role: Option<Role>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " · Lineup Generator" }
                // Tailwind via CDN (dev-only; swap for a built CSS file
                // once we add a pipeline — see DESIGN.md defer list).
                script src="https://cdn.tailwindcss.com" {}
                script src="/htmx.min.js" {}
                script src="/alpine.min.js" defer {}
                // Hide x-cloak elements until Alpine initializes
                style { "[x-cloak] { display: none !important; }" }
                // Highlight the active nav link on page load and HTMX
                // navigation. Prefix-matches location.pathname against
                // data-nav attributes, with alias grouping for related
                // paths (e.g. /history, /solve → Practices).
                script {
                    (maud::PreEscaped(include_str!("js/active_nav.js")))
                }
                // Print-friendly overrides: hide chrome, expand content
                style { r#"
                    @media print {
                        nav, .no-print { display: none !important; }
                        body { background: white !important; }
                        main { padding: 0 !important; }
                        .bg-white { box-shadow: none !important; }
                        /* Page-break between boat cards */
                        .print-break { break-inside: avoid; page-break-inside: avoid; }
                        /* Remove sticky positioning */
                        .sticky { position: static !important; }
                        /* Ensure text is black for readability */
                        * { color-adjust: exact; -webkit-print-color-adjust: exact; }
                    }
                "# }
            }
            body class="bg-slate-50 text-slate-900 min-h-screen flex flex-col" {
                (navbar(role))
                main #content class="flex-grow" {
                    (content)
                }
            }
        }
    }
}

fn navbar(role: Option<Role>) -> Markup {
    let r = role.unwrap_or(Role::Member);
    let is_coach = r.at_least(Role::Coach);
    let is_pd = r.at_least(Role::ProgramDirector);

    html! {
        nav class="bg-slate-800 text-white px-4 sm:px-6 py-3 sticky top-0 z-40 shadow"
             x-data="{ open: false }" {
            // Top bar: team name + hamburger on mobile, full links on desktop
            div class="flex items-center justify-between" {
                // Team name (left)
                div class="font-bold text-lg"
                    hx-get="/teams/selector"
                    hx-trigger="load"
                    hx-swap="innerHTML" {
                }

                // Desktop nav links (hidden on mobile)
                ul class="hidden lg:flex items-center space-x-4" {
                    (nav_link("/practices", "Practices"))
                    @if is_coach {
                        (nav_link("/boats", "Fleet"))
                        (nav_link("/rowers", "Roster"))
                        (nav_link("/sync", "Sync"))
                    }
                    @if is_pd {
                        (nav_link("/teams", "Teams"))
                        (nav_link("/users", "Users"))
                    }
                    li class="ml-4" {}
                    (nav_link("/my/profile", "Profile"))
                    (nav_link("/my/availability", "Availability"))
                    (nav_link("/my/email-preferences", "Email"))
                    li {
                        form method="post" action="/logout" class="inline" {
                            button type="submit" class="px-3 py-2 rounded hover:bg-white/10 transition text-sm" {
                                "Logout"
                            }
                        }
                    }
                }

                // Hamburger button (mobile only)
                button class="lg:hidden p-2 rounded hover:bg-white/10"
                       "@click"="open = !open"
                       aria-label="Menu" {
                    // Hamburger icon
                    div class="w-5 h-0.5 bg-white mb-1" {}
                    div class="w-5 h-0.5 bg-white mb-1" {}
                    div class="w-5 h-0.5 bg-white" {}
                }
            }

            // Mobile menu (toggles)
            ul class="lg:hidden flex-col space-y-1 pt-3"
               x-show="open"
               x-cloak
               "@click"="open = false" {
                (nav_link("/practices", "Practices"))
                @if is_coach {
                    (nav_link("/boats", "Fleet"))
                    (nav_link("/rowers", "Roster"))
                    (nav_link("/sync", "Sync"))
                }
                @if is_pd {
                    (nav_link("/teams", "Teams"))
                    (nav_link("/users", "Users"))
                }
                li class="border-t border-slate-700 my-1 pt-1" {}
                (nav_link("/my/profile", "Profile"))
                (nav_link("/my/availability", "Availability"))
                (nav_link("/my/email-preferences", "Email"))
                li {
                    form method="post" action="/logout" class="inline" {
                        button type="submit" class="block w-full text-left px-3 py-2 rounded hover:bg-white/10 transition text-sm" {
                            "Logout"
                        }
                    }
                }
            }
        }
    }
}

fn nav_link(href: &str, label: &str) -> Markup {
    html! {
        li {
            a href=(href)
              class="px-3 py-2 rounded hover:bg-white/10 transition cursor-pointer"
              data-nav=(href)
              hx-get=(href)
              hx-target="#content"
              hx-push-url="true"
              { (label) }
        }
    }
}

/// Tiny empty-state helper used by a few list views.
pub(crate) fn empty_state(message: &str) -> Markup {
    html! {
        div class="text-center text-slate-500 italic py-12" { (message) }
    }
}

/// Generic page header: large title + optional subtitle.
pub(crate) fn page_header(title: &str, subtitle: Option<&str>) -> Markup {
    html! {
        header class="bg-white border-b border-slate-200 px-4 sm:px-8 py-4 sm:py-6" {
            h1 class="text-2xl font-bold text-slate-800" { (title) }
            @if let Some(sub) = subtitle {
                p class="text-sm text-slate-500 mt-1" { (sub) }
            }
        }
    }
}
