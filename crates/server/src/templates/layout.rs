//! Base page chrome: `<html>` + `<head>` + navbar + `#content` shell.
//!
//! Loads HTMX, Alpine.js, and Tailwind from paths that [`ServeDir`]
//! resolves out of `crates/server/public` in dev. Tailwind comes from
//! the CDN until we add a build pipeline.
//!
//! [`ServeDir`]: tower_http::services::ServeDir

use maud::{html, Markup, DOCTYPE};

use lineup_db::app_user::Role;

pub(crate) fn page(title: &str, content: Markup, role: Role, is_superuser: bool) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " · Oarful" }
                // Pre-built Tailwind CSS. Rebuild with:
                //   tailwindcss -i tailwind.input.css -o crates/server/public/tailwind.css --minify
                link rel="stylesheet" href="/tailwind.css";
                script src="/htmx.min.js" {}
                script src="/alpine.min.js" defer {}
                script src="/htmx-ext-sse.js" defer {}
                // Hide x-cloak elements until Alpine initializes
                style { r#"
                    [x-cloak] { display: none !important; }
                    .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0,0,0,0); white-space: nowrap; border: 0; }
                    .skip-link { position: absolute; left: -9999px; top: auto; width: 1px; height: 1px; overflow: hidden; z-index: 100; }
                    .skip-link:focus { left: 8px; top: 8px; width: auto; height: auto; padding: 8px 16px; background: var(--ink); color: var(--paper); border-radius: 4px; font-weight: 600; text-decoration: none; }
                "# }
                // Highlight the active nav link on page load and HTMX
                // navigation. Prefix-matches location.pathname against
                // data-nav attributes, with alias grouping for related
                // paths (e.g. /history, /solve → Practices).
                script {
                    (maud::PreEscaped(include_str!("js/active_nav.js")))
                }
                // Respect reduced-motion preference
                style { r#"
                    @media (prefers-reduced-motion: reduce) {
                        *, *::before, *::after {
                            animation-duration: 0.01ms !important;
                            animation-iteration-count: 1 !important;
                            transition-duration: 0.01ms !important;
                            scroll-behavior: auto !important;
                        }
                    }
                "# }
                // Print-friendly overrides: hide chrome, expand content
                style { r#"
                    @media print {
                        nav, .no-print { display: none !important; }
                        body { background: white !important; }
                        main { padding: 0 !important; }
                        .bg-paper { box-shadow: none !important; }
                        /* Page-break between boat cards */
                        .print-break { break-inside: avoid; page-break-inside: avoid; }
                        /* Remove sticky positioning */
                        .sticky { position: static !important; }
                        /* Ensure text is black for readability */
                        * { color-adjust: exact; -webkit-print-color-adjust: exact; }
                    }
                "# }
            }
            body class="min-h-screen flex flex-col bg-paper text-ink" {
                a href="#content" class="skip-link" { "Skip to content" }
                @if is_superuser {
                    div class="bg-warn text-paper text-center text-sm py-1.5 px-4 no-print" {
                        "Impersonating this tenant "
                        form method="post" action="/su/exit" class="inline" {
                            button type="submit"
                                   class="underline font-semibold hover:text-amber-100 transition" {
                                "Exit"
                            }
                        }
                    }
                }
                (navbar(role))
                main #content class="flex-grow" "aria-live"="polite" {
                    (content)
                }
                footer class="text-center text-xs py-2 no-print text-muted bg-paper border-t"
                       style="border-color: var(--rule)" {
                    a href=(source_url()) target="_blank"
                      class="transition text-muted" {
                        "Source code (AGPL-3.0)"
                    }
                }
                // Unified toast (fixed bottom-left, hidden by default)
                div #toast
                    class="fixed bottom-4 left-4 z-50 max-w-sm hidden"
                    role="status" {
                    div #toast-inner
                        class="px-4 py-3 rounded-lg shadow-lg flex items-start gap-3 text-sm border border-l-4" {
                        span #toast-msg class="flex-1" {}
                        button type="button"
                               class="font-bold text-lg leading-none opacity-60 hover:opacity-100"
                               "aria-label"="Dismiss"
                               onclick="document.getElementById('toast').classList.add('hidden')" {
                            span "aria-hidden"="true" { "\u{00d7}" }
                        }
                    }
                }
                script {
                    (maud::PreEscaped(include_str!("js/focus_trap.js")))
                }
                script {
                    (maud::PreEscaped(include_str!("js/toast.js")))
                }
                script {
                    (maud::PreEscaped(include_str!("js/tab_cookies.js")))
                }
                // lineup editor Alpine component + generate animation
                script {
                    (maud::PreEscaped(include_str!("js/lineup_editor.js")))
                }
            }
        }
    }
}

fn navbar(role: Role) -> Markup {
    let r = role;
    let is_coach = r.at_least(Role::Coach);
    let is_pd = r.at_least(Role::ProgramDirector);

    html! {
        nav class="bg-ink text-paper px-4 sm:px-6 py-3 sticky top-0 z-40 shadow"
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
                    (nav_link_with_badge("/practices", "Practices", is_coach))
                    @if is_coach {
                        (nav_link("/team", "Team"))
                    }
                    @if is_pd {
                        (nav_link("/admin", "Admin"))
                    }
                    li class="ml-4" {}
                    (nav_link("/my", "My"))
                    li {
                        form method="post" action="/logout" class="inline" {
                            button type="submit" class="px-3 py-2 rounded hover:bg-paper/10 transition text-sm" {
                                "Logout"
                            }
                        }
                    }
                }

                // Hamburger button (mobile only)
                button class="lg:hidden p-2 rounded hover:bg-paper/10"
                       "@click"="open = !open"
                       ":aria-expanded"="open"
                       aria-label="Menu" {
                    // Hamburger icon
                    div class="w-5 h-0.5 bg-paper mb-1" {}
                    div class="w-5 h-0.5 bg-paper mb-1" {}
                    div class="w-5 h-0.5 bg-paper" {}
                }
            }

            // Mobile menu (toggles)
            ul class="lg:hidden flex-col space-y-1 pt-3"
               x-show="open"
               x-cloak
               "@click"="open = false" {
                (nav_link_with_badge("/practices", "Practices", is_coach))
                @if is_coach {
                    (nav_link("/team", "Team"))
                }
                @if is_pd {
                    (nav_link("/admin", "Admin"))
                }
                li class="border-t border-ink-3 my-1 pt-1" {}
                (nav_link("/my", "My"))
                li {
                    form method="post" action="/logout" class="inline" {
                        button type="submit" class="block w-full text-left px-3 py-2 rounded hover:bg-paper/10 transition text-sm" {
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
              class="px-3 py-2 rounded hover:bg-paper/10 transition cursor-pointer"
              data-nav=(href)
              hx-get=(href)
              hx-target="#content"
              hx-push-url="true"
              { (label) }
        }
    }
}

fn nav_link_with_badge(href: &str, label: &str, show_badge: bool) -> Markup {
    html! {
        li {
            a href=(href)
              class="px-3 py-2 rounded hover:bg-paper/10 transition cursor-pointer inline-flex items-center"
              data-nav=(href)
              hx-get=(href)
              hx-target="#content"
              hx-push-url="true"
            {
                (label)
                @if show_badge {
                    span hx-get="/nav/stale-badge"
                         hx-trigger="load"
                         hx-swap="innerHTML"
                         hx-push-url="false"
                         hx-target="this" {}
                }
            }
        }
    }
}

/// Tab definition for the shared tabbed section.
pub(crate) struct TabDef {
    pub label: &'static str,
    pub url: &'static str,
    pub id: &'static str,
}

/// Shared tabbed page wrapper used by `/team` and `/admin`.
/// Each tab's content already includes its own header, so this only
/// renders the tab bar + a swap target div.
pub(crate) fn tabbed_section(
    tabs: &[TabDef],
    active_tab: &str,
    target_id: &str,
    tab_content: Markup,
) -> Markup {
    let bar_id = format!("{target_id}-bar");
    html! {
        div id=(bar_id)
            class="border-b border-rule-2 bg-paper px-4 sm:px-8 pt-3" {
            div class="flex gap-1" {
                @for tab in tabs {
                    (section_tab(tab, active_tab, target_id))
                }
            }
        }
        div id=(target_id) "aria-live"="polite" {
            (tab_content)
        }
    }
}

/// Render tab content + an OOB swap of the tab bar for HTMX tab
/// switches. The content replaces the target div normally; the tab
/// bar is swapped out-of-band so its active state updates.
pub(crate) fn tab_swap(
    tabs: &[TabDef],
    active_tab: &str,
    target_id: &str,
    tab_content: Markup,
) -> Markup {
    let bar_id = format!("{target_id}-bar");
    html! {
        // Main swap: replaces the tab content div
        (tab_content)
        // OOB swap: updates the tab bar with new active state
        div id=(bar_id) hx-swap-oob="true"
            class="border-b border-rule-2 bg-paper px-4 sm:px-8 pt-3" {
            div class="flex gap-1" {
                @for tab in tabs {
                    (section_tab(tab, active_tab, target_id))
                }
            }
        }
    }
}

fn section_tab(tab: &TabDef, active: &str, target_id: &str) -> Markup {
    let is_active = tab.id == active;
    let base = "px-4 py-2 text-sm font-medium border-b-2 transition cursor-pointer";
    let classes = if is_active {
        format!("{base} border-ink text-ink")
    } else {
        format!("{base} border-transparent text-ink-3 hover:text-ink-2 hover:border-rule")
    };
    html! {
        button hx-get=(tab.url)
               hx-target=(format!("#{target_id}"))
               hx-push-url="true"
               class=(classes) {
            (tab.label)
        }
    }
}

/// Tiny empty-state helper used by a few list views.
pub(crate) fn empty_state(message: &str) -> Markup {
    html! {
        div class="text-center text-ink-3 italic py-12" { (message) }
    }
}

fn source_url() -> String {
    std::env::var("SOURCE_URL")
        .unwrap_or_else(|_| "https://github.com/hgzimmerman/oarful".to_string())
}

/// Generic page header: large title + optional subtitle.
pub(crate) fn page_header(title: &str, subtitle: Option<&str>) -> Markup {
    html! {
        header class="bg-paper border-b border-rule-2 px-4 sm:px-8 py-4 sm:py-6" {
            h1 class="text-2xl font-bold text-ink font-serif-heading" { (title) }
            @if let Some(sub) = subtitle {
                p class="text-sm text-muted mt-1" { (sub) }
            }
        }
    }
}
