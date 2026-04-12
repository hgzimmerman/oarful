//! Team selector dropdown + management pages.

use lineup_db::team::{Team, TeamId};
use maud::{html, Markup};

use super::layout::page_header;

/// Renders a compact team switcher that auto-submits on change.
/// Sits in the navbar's right side. When only one team exists, shows
/// the name as plain text (no dropdown) since there's nothing to
/// switch to.
pub(crate) fn selector(teams: &[Team], active: TeamId, tenant_name: Option<&str>) -> Markup {
    let tenant_prefix = tenant_name.map(|n| html! {
        span class="text-xs text-slate-400 mr-2 hidden 2xl:inline" { (n) " ·" }
    });

    if teams.len() <= 1 {
        let name = teams
            .first()
            .map(|t| t.name.as_str())
            .unwrap_or("No team");
        return html! {
            span class="flex items-center" {
                @if let Some(prefix) = tenant_prefix { (prefix) }
                span class="text-sm text-slate-300" { (name) }
            }
        };
    }

    html! {
        form method="post" action="/switch-team"
             class="flex items-center space-x-2" {
            @if let Some(prefix) = tenant_prefix { (prefix) }
            label class="text-xs text-slate-400 uppercase tracking-wide" { "Team" }
            select name="team_id"
                   onchange="this.form.submit()"
                   class="bg-slate-700 text-white text-sm rounded px-2 py-1 border border-slate-600 focus:border-slate-400 focus:outline-none cursor-pointer" {
                @for t in teams {
                    @if t.id == active {
                        option value=(t.id) selected { (t.name) }
                    } @else {
                        option value=(t.id) { (t.name) }
                    }
                }
            }
        }
    }
}

// =====================================================================
// Team management (PD only)
// =====================================================================

pub(crate) fn list_content(teams: &[Team]) -> Markup {
    let subtitle = format!("{} teams", teams.len());
    html! {
        (page_header("Teams", Some(&subtitle)))
        div class="px-4 sm:px-8 py-6 max-w-3xl mx-auto space-y-6" {
            // Create team form
            form method="post" action="/teams"
                 hx-post="/teams"
                 hx-target="#content"
                 hx-push-url="true"
                 class="flex items-end gap-3" {
                div {
                    label for="team_name" class="block text-xs font-semibold text-slate-700 uppercase tracking-wide mb-1" {
                        "New team"
                    }
                    input id="team_name" name="name" type="text" required placeholder="Team name"
                          class="border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                }
                button type="submit"
                       class="bg-slate-800 hover:bg-slate-900 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                    "Create"
                }
            }

            @if teams.is_empty() {
                div class="text-slate-500 italic" { "No teams." }
            } @else {
                div class="bg-white rounded-lg shadow divide-y divide-slate-200" {
                    @for t in teams {
                        a href={"/teams/" (t.id)}
                          hx-get={"/teams/" (t.id)}
                          hx-target="#content"
                          hx-push-url="true"
                          class="flex items-center justify-between px-6 py-4 hover:bg-slate-50 transition cursor-pointer" {
                            div {
                                div class="font-semibold text-slate-800" { (t.name) }
                                div class="text-sm text-slate-500" {
                                    "Self-edit: " (t.self_edit_level)
                                }
                            }
                            span class="text-slate-400" { "→" }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn detail_content(team: &Team) -> Markup {
    let action = format!("/teams/{}", team.id);
    html! {
        (page_header(&team.name, Some("Team settings")))
        div class="px-4 sm:px-8 py-6 max-w-2xl mx-auto space-y-6" {
            a href="/teams"
              hx-get="/teams"
              hx-target="#content"
              hx-push-url="true"
              class="text-sm text-slate-500 hover:text-slate-800" {
                "← back to teams"
            }

            form method="post" action=(action)
                 hx-post=(action)
                 hx-target="#content"
                 class="bg-white rounded-lg shadow p-6 space-y-4" {
                div {
                    label for="name" class="block text-sm font-semibold text-slate-700 mb-1" {
                        "Team name"
                    }
                    input id="name" name="name" type="text" required
                          value=(team.name)
                          class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                }
                div {
                    label for="self_edit_level" class="block text-sm font-semibold text-slate-700 mb-1" {
                        "Member self-edit level"
                    }
                    select id="self_edit_level" name="self_edit_level"
                           class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none" {
                        option value="low" selected[team.self_edit_level == "low"] {
                            "Low — side, cox, scull only"
                        }
                        option value="medium" selected[team.self_edit_level == "medium"] {
                            "Medium — + height"
                        }
                        option value="high" selected[team.self_edit_level == "high"] {
                            "High — all attributes (except active)"
                        }
                    }
                    p class="text-xs text-slate-500 mt-1" {
                        "Controls which attributes members can edit on their own profile. Coach+ always has full access."
                    }
                }
                button type="submit"
                       class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition" {
                    "Save"
                }
            }
        }
    }
}
