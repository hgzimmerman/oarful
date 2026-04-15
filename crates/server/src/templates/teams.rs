//! Team selector dropdown + management pages.

use std::collections::HashSet;

use lineup_db::boat::Boat;
use lineup_db::boat::types::BoatId;
use lineup_db::rower::Rower;
use lineup_db::rower::types::RowerId;
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
                                div class="font-semibold text-slate-800" {
                                    (t.name)
                                    @if t.archived.as_bool() {
                                        span class="ml-2 text-xs font-normal text-red-500" { "(archived)" }
                                    }
                                }
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
                div class="grid grid-cols-1 sm:grid-cols-2 gap-4" {
                    div {
                        label for="default_practice_time" class="block text-sm font-semibold text-slate-700 mb-1" {
                            "Default practice time"
                        }
                        @let time_value = team.default_practice_time.map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
                        input id="default_practice_time" name="default_practice_time" type="time"
                              value=(time_value)
                              class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                        p class="text-xs text-slate-500 mt-1" {
                            "Pre-fills the time when creating new practices."
                        }
                    }
                    div {
                        label for="default_practice_duration" class="block text-sm font-semibold text-slate-700 mb-1" {
                            "Default duration (minutes)"
                        }
                        @let dur_value = team.default_practice_duration_minutes.map(|m| m.to_string()).unwrap_or_default();
                        input id="default_practice_duration" name="default_practice_duration_minutes" type="number"
                              min="1" step="1"
                              value=(dur_value)
                              placeholder="e.g. 90"
                              class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                        p class="text-xs text-slate-500 mt-1" {
                            "Used for cross-team overlap detection."
                        }
                    }
                }
                button type="submit"
                       class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition" {
                    "Save"
                }
            }

            // Archive / unarchive section
            section class="border-t border-red-200 pt-4" {
                @if team.archived.as_bool() {
                    div class="flex items-center gap-3" {
                        span class="text-sm text-red-600 font-medium" { "This team is archived." }
                        form method="post" action={"/teams/" (team.id) "/toggle-archive"}
                             hx-post={"/teams/" (team.id) "/toggle-archive"}
                             hx-target="#content" {
                            button type="submit"
                                   class="text-sm text-emerald-600 hover:text-emerald-800 font-medium py-2" {
                                "Unarchive"
                            }
                        }
                    }
                } @else {
                    form method="post" action={"/teams/" (team.id) "/toggle-archive"}
                         hx-post={"/teams/" (team.id) "/toggle-archive"}
                         hx-target="#content"
                         hx-confirm="Archive this team? It will be hidden from the team switcher for non-PD users." {
                        button type="submit"
                               class="text-sm text-red-600 hover:text-red-800 font-medium py-2" {
                            "Archive team"
                        }
                    }
                }
            }
        }
    }
}

// =====================================================================
// Roster assignment matrix — rowers × teams
// =====================================================================

pub(crate) fn roster_matrix(
    rowers: &[Rower],
    teams: &[Team],
    memberships: &HashSet<(TeamId, RowerId)>,
) -> Markup {
    roster_matrix_inner(None, rowers, teams, memberships)
}

pub(crate) fn roster_matrix_with_toast(
    message: &str,
    rowers: &[Rower],
    teams: &[Team],
    memberships: &HashSet<(TeamId, RowerId)>,
) -> Markup {
    roster_matrix_inner(Some(message), rowers, teams, memberships)
}

fn roster_matrix_inner(
    toast: Option<&str>,
    rowers: &[Rower],
    teams: &[Team],
    memberships: &HashSet<(TeamId, RowerId)>,
) -> Markup {
    let subtitle = format!("{} rowers · {} teams", rowers.len(), teams.len());
    html! {
        (page_header("Roster assignments", Some(&subtitle)))
        div class="px-4 sm:px-8 py-6" {
            @if let Some(msg) = toast {
                div class="bg-emerald-50 border border-emerald-200 text-emerald-800 rounded-lg px-6 py-4 text-sm mb-4" {
                    (msg)
                }
            }
            @if teams.is_empty() {
                div class="text-slate-500 italic" { "No teams. Create teams first." }
            } @else if rowers.is_empty() {
                div class="text-slate-500 italic" { "No active rowers." }
            } @else {
                form method="post" action="/admin/roster"
                     hx-post="/admin/roster"
                     hx-target="#admin-tab-content" {
                    div class="flex justify-end mb-3" {
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                            "Save"
                        }
                    }
                    div class="overflow-auto bg-white rounded-lg shadow max-h-[75vh]" {
                        table class="text-xs border-collapse" {
                            thead {
                                tr {
                                    th class="sticky top-0 left-0 z-20 bg-slate-100 px-3 py-2 text-left font-semibold text-slate-700 border-b border-r border-slate-200 min-w-[160px]" {
                                        "Rower"
                                    }
                                    @for team in teams {
                                        th class="sticky top-0 z-10 bg-slate-100 px-3 py-2 text-center font-semibold text-slate-700 border-b border-slate-200 whitespace-nowrap min-w-[80px]" {
                                            (team.name)
                                        }
                                    }
                                }
                            }
                            tbody {
                                @for rower in rowers {
                                    tr class="border-t border-slate-100 hover:bg-slate-50" {
                                        td class="sticky left-0 z-10 bg-white px-3 py-1.5 font-medium text-slate-800 border-r border-slate-200 whitespace-nowrap" {
                                            a href={"/rowers/" (rower.id)}
                                              hx-get={"/rowers/" (rower.id)}
                                              hx-target="#content"
                                              hx-push-url="true"
                                              class="text-blue-700 hover:text-blue-900" {
                                                (rower.name)
                                            }
                                        }
                                        @for team in teams {
                                            td class="text-center border-slate-100 px-1" {
                                                @let field_name = format!("m_{}_{}", team.id, rower.id);
                                                @let checked = memberships.contains(&(team.id, rower.id));
                                                input type="checkbox"
                                                       name=(field_name)
                                                       value="1"
                                                       checked[checked]
                                                       class="w-4 h-4 accent-emerald-600 cursor-pointer";
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div class="flex justify-end mt-3" {
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                            "Save"
                        }
                    }
                }
            }
        }
    }
}

// =====================================================================
// Fleet assignment matrix — boats × teams
// =====================================================================

pub(crate) fn fleet_matrix(
    boats: &[Boat],
    teams: &[Team],
    defaults: &HashSet<(TeamId, BoatId)>,
) -> Markup {
    fleet_matrix_inner(None, boats, teams, defaults)
}

pub(crate) fn fleet_matrix_with_toast(
    message: &str,
    boats: &[Boat],
    teams: &[Team],
    defaults: &HashSet<(TeamId, BoatId)>,
) -> Markup {
    fleet_matrix_inner(Some(message), boats, teams, defaults)
}

fn fleet_matrix_inner(
    toast: Option<&str>,
    boats: &[Boat],
    teams: &[Team],
    defaults: &HashSet<(TeamId, BoatId)>,
) -> Markup {
    let subtitle = format!("{} sweep boats · {} teams", boats.len(), teams.len());
    html! {
        (page_header("Default fleet", Some(&subtitle)))
        div class="px-4 sm:px-8 py-6" {
            @if let Some(msg) = toast {
                div class="bg-emerald-50 border border-emerald-200 text-emerald-800 rounded-lg px-6 py-4 text-sm mb-4" {
                    (msg)
                }
            }
            p class="text-sm text-slate-500 mb-4" {
                "Select which boats are pre-selected in the generation pool for each team. "
                "Single-team tenants default to all boats if none are selected."
            }
            @if teams.is_empty() {
                div class="text-slate-500 italic" { "No teams. Create teams first." }
            } @else if boats.is_empty() {
                div class="text-slate-500 italic" { "No sweep boats in the fleet." }
            } @else {
                form method="post" action="/admin/fleet/defaults"
                     hx-post="/admin/fleet/defaults"
                     hx-target="#admin-fleet-content" {
                    div class="flex justify-end mb-3" {
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                            "Save"
                        }
                    }
                    div class="overflow-auto bg-white rounded-lg shadow max-h-[75vh]" {
                        table class="text-xs border-collapse" {
                            thead {
                                tr {
                                    th class="sticky top-0 left-0 z-20 bg-slate-100 px-3 py-2 text-left font-semibold text-slate-700 border-b border-r border-slate-200 min-w-[160px]" {
                                        "Boat"
                                    }
                                    @for team in teams {
                                        th class="sticky top-0 z-10 bg-slate-100 px-3 py-2 text-center font-semibold text-slate-700 border-b border-slate-200 whitespace-nowrap min-w-[80px]" {
                                            (team.name)
                                        }
                                    }
                                }
                            }
                            tbody {
                                @for boat in boats {
                                    tr class="border-t border-slate-100 hover:bg-slate-50" {
                                        td class="sticky left-0 z-10 bg-white px-3 py-1.5 font-medium text-slate-800 border-r border-slate-200 whitespace-nowrap" {
                                            a href={"/boats/" (boat.id)}
                                              hx-get={"/boats/" (boat.id)}
                                              hx-target="#content"
                                              hx-push-url="true"
                                              class="text-blue-700 hover:text-blue-900" {
                                                (boat.name)
                                            }
                                            span class="text-slate-400 ml-1" {
                                                "(" (boat.seat_count)
                                                @if boat.has_cox.as_bool() { "+" }
                                                ")"
                                            }
                                        }
                                        @for team in teams {
                                            td class="text-center border-slate-100 px-1" {
                                                @let field_name = format!("b_{}_{}", team.id, boat.id);
                                                @let checked = defaults.contains(&(team.id, boat.id));
                                                input type="checkbox"
                                                       name=(field_name)
                                                       value="1"
                                                       checked[checked]
                                                       class="w-4 h-4 accent-emerald-600 cursor-pointer";
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div class="flex justify-end mt-3" {
                        button type="submit"
                               class="bg-emerald-600 hover:bg-emerald-700 text-white font-semibold px-4 py-2 rounded shadow transition text-sm" {
                            "Save"
                        }
                    }
                }
            }
        }
    }
}
