//! Team selector dropdown for the navbar.

use lineup_db::team::{Team, TeamId};
use maud::{html, Markup};

/// Renders a compact team switcher that auto-submits on change.
/// Sits in the navbar's right side. When only one team exists, shows
/// the name as plain text (no dropdown) since there's nothing to
/// switch to.
pub(crate) fn selector(teams: &[Team], active: TeamId) -> Markup {
    if teams.len() <= 1 {
        // Single team — no switcher needed, just show the name.
        let name = teams
            .first()
            .map(|t| t.name.as_str())
            .unwrap_or("No team");
        return html! {
            span class="text-sm text-slate-300" { (name) }
        };
    }

    html! {
        form method="post" action="/switch-team"
             class="flex items-center space-x-2" {
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
