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
        nav class="bg-slate-800 text-white px-6 py-3 sticky top-0 z-40 shadow" {
            ul class="flex items-center space-x-6" {
                // Team name as the nav title — loaded via HTMX.
                li class="font-bold text-lg mr-4"
                   hx-get="/teams/selector"
                   hx-trigger="load"
                   hx-swap="innerHTML" {
                }

                // Everyone sees practices
                (nav_link("/practices", "Practices"))

                // Coach+ sees fleet, roster, sync
                @if is_coach {
                    (nav_link("/boats", "Fleet"))
                    (nav_link("/rowers", "Roster"))
                    (nav_link("/sync", "Sync"))
                }

                // PD only
                @if is_pd {
                    (nav_link("/teams", "Teams"))
                    (nav_link("/users", "Users"))
                }

                // Self-service — everyone
                li class="ml-auto" {}
                (nav_link("/my/profile", "Profile"))
                (nav_link("/my/availability", "Availability"))

                // Logout
                li {
                    form method="post" action="/logout" class="inline" {
                        button type="submit" class="px-3 py-2 rounded hover:bg-white/10 transition text-sm" {
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
        header class="bg-white border-b border-slate-200 px-8 py-6" {
            h1 class="text-2xl font-bold text-slate-800" { (title) }
            @if let Some(sub) = subtitle {
                p class="text-sm text-slate-500 mt-1" { (sub) }
            }
        }
    }
}
