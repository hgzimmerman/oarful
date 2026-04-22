//! Audit log list view with filters and "load more" pagination.

use std::collections::HashMap;

use lineup_db::app_user::UserId;
use lineup_db::audit_log::AuditLog;
use maud::{html, Markup};

use crate::handlers::audit::{AuditQuery, PAGE_SIZE};

pub(crate) fn list_content(
    entries: &[AuditLog],
    actions: &[String],
    user_map: &HashMap<UserId, String>,
    query: &AuditQuery,
    offset: i64,
    has_more: bool,
) -> Markup {
    html! {
        header class="bg-white border-b border-slate-200 px-4 sm:px-8 py-4 sm:py-6" {
            h1 class="text-2xl font-bold text-slate-800" { "Audit log" }
            p class="text-sm text-slate-500 mt-1" { "90-day history of changes" }
        }

        div class="px-4 sm:px-8 py-6 space-y-4 max-w-6xl mx-auto" {
            // Filters
            (filter_bar(actions, user_map, query))

            // Table
            @if entries.is_empty() && offset == 0 {
                div class="text-center text-slate-500 italic py-12" {
                    "No audit entries found."
                }
            } @else {
                div class="bg-white rounded-lg shadow overflow-hidden" {
                    table class="w-full text-sm" {
                        thead class="bg-slate-100 text-left text-xs uppercase text-slate-600" {
                            tr {
                                th class="px-4 py-2" { "When" }
                                th class="px-4 py-2" { "User" }
                                th class="px-4 py-2" { "Action" }
                                th class="px-4 py-2" { "Resource" }
                                th class="px-4 py-2" { "Detail" }
                            }
                        }
                        tbody #audit-rows {
                            (rows_fragment(entries, user_map))
                        }
                    }
                }

                // Load more
                div #audit-load-more {
                    @if has_more {
                        (load_more_button(query, offset))
                    }
                }
            }
        }
    }
}

/// Render just the new rows + an updated load-more button. Used by the
/// HTMX "load more" endpoint. The rows swap into `#audit-rows` via
/// `beforeend` and the button replaces `#audit-load-more`.
pub(crate) fn rows_and_load_more(
    entries: &[AuditLog],
    user_map: &HashMap<UserId, String>,
    query: &AuditQuery,
    offset: i64,
    has_more: bool,
) -> Markup {
    html! {
        // Append these rows into the tbody
        (rows_fragment(entries, user_map))

        // Out-of-band swap to replace the load-more div
        div #audit-load-more hx-swap-oob="true" {
            @if has_more {
                (load_more_button(query, offset))
            }
        }
    }
}

fn rows_fragment(entries: &[AuditLog], user_map: &HashMap<UserId, String>) -> Markup {
    html! {
        @for entry in entries {
            tr class="border-t border-slate-100 hover:bg-slate-50" {
                td class="px-4 py-2 text-xs text-slate-500 whitespace-nowrap" {
                    (entry.timestamp.format("%Y-%m-%d %H:%M"))
                }
                td class="px-4 py-2" {
                    @if let Some(uid) = entry.user_id {
                        @if let Some(name) = user_map.get(&uid) {
                            span class="text-slate-800" { (name) }
                        } @else {
                            span class="text-slate-400" { "user #" (uid) }
                        }
                    } @else {
                        span class="text-slate-400 italic" { "system" }
                    }
                }
                td class="px-4 py-2" {
                    (action_badge(&entry.action))
                }
                td class="px-4 py-2 text-xs font-mono text-slate-600" {
                    (entry.resource_type) "/" (entry.resource_id)
                }
                td class="px-4 py-2 text-xs text-slate-500 max-w-xs truncate" {
                    @if let Some(ref d) = entry.detail {
                        (d)
                    }
                }
            }
        }
    }
}

fn action_badge(action: &str) -> Markup {
    let (bg, text) = match action.split('.').next().unwrap_or("") {
        "lineup" => ("bg-blue-100", "text-blue-800"),
        "rower" => ("bg-emerald-100", "text-emerald-800"),
        "boat" => ("bg-amber-100", "text-amber-800"),
        "practice" => ("bg-violet-100", "text-violet-800"),
        "invite" => ("bg-pink-100", "text-pink-800"),
        "sync" => ("bg-cyan-100", "text-cyan-800"),
        "solver_profile" => ("bg-orange-100", "text-orange-800"),
        "team" => ("bg-indigo-100", "text-indigo-800"),
        "availability" => ("bg-teal-100", "text-teal-800"),
        _ => ("bg-slate-100", "text-slate-800"),
    };
    html! {
        span class=(format!("{bg} {text} text-xs px-1.5 py-0.5 rounded-full")) {
            (action)
        }
    }
}

fn filter_bar(
    actions: &[String],
    user_map: &HashMap<UserId, String>,
    query: &AuditQuery,
) -> Markup {
    let sel_action = query.action.as_deref().unwrap_or("");
    let sel_user = query.user_id.map(|u| u.to_string()).unwrap_or_default();
    let sel_resource = query.resource_type.as_deref().unwrap_or("");

    // Collect sorted user list for dropdown
    let mut users: Vec<(UserId, &str)> = user_map
        .iter()
        .map(|(id, name)| (*id, name.as_str()))
        .collect();
    users.sort_by(|a, b| a.1.cmp(b.1));

    let resource_types = [
        "availability",
        "boat",
        "practice",
        "rower",
        "solver_profile",
        "sync_source",
        "team",
        "user",
    ];

    html! {
        form method="get" action="/admin/audit"
             hx-get="/admin/audit"
             hx-target="#content"
             hx-push-url="true"
             class="flex flex-wrap items-end gap-3" {

            div {
                label class="block text-xs text-slate-500 mb-1" { "Action" }
                select name="action"
                       class="border border-slate-300 rounded px-2 py-1.5 text-sm" {
                    option value="" { "All actions" }
                    @for a in actions {
                        @if a.as_str() == sel_action {
                            option value=(a) selected { (a) }
                        } @else {
                            option value=(a) { (a) }
                        }
                    }
                }
            }

            div {
                label class="block text-xs text-slate-500 mb-1" { "User" }
                select name="user_id"
                       class="border border-slate-300 rounded px-2 py-1.5 text-sm" {
                    option value="" { "All users" }
                    option value="-1" selected[sel_user == "-1"] { "System" }
                    @for (uid, name) in &users {
                        @if uid.to_string() == sel_user {
                            option value=(uid) selected { (name) }
                        } @else {
                            option value=(uid) { (name) }
                        }
                    }
                }
            }

            div {
                label class="block text-xs text-slate-500 mb-1" { "Resource" }
                select name="resource_type"
                       class="border border-slate-300 rounded px-2 py-1.5 text-sm" {
                    option value="" { "All resources" }
                    @for rt in &resource_types {
                        @if *rt == sel_resource {
                            option value=(rt) selected { (rt) }
                        } @else {
                            option value=(rt) { (rt) }
                        }
                    }
                }
            }

            button type="submit"
                   class="bg-slate-700 hover:bg-slate-800 text-white text-sm font-semibold px-4 py-1.5 rounded transition" {
                "Filter"
            }

            @if query.action.is_some() || query.user_id.is_some() || query.resource_type.is_some() {
                a href="/admin/audit"
                  hx-get="/admin/audit"
                  hx-target="#content"
                  hx-push-url="true"
                  class="text-sm text-slate-500 hover:text-slate-800" {
                    "Clear"
                }
            }
        }
    }
}

fn load_more_button(query: &AuditQuery, offset: i64) -> Markup {
    let next_offset = offset + PAGE_SIZE;
    let mut params = vec![format!("offset={next_offset}")];
    if let Some(ref a) = query.action {
        if !a.is_empty() {
            params.push(format!("action={a}"));
        }
    }
    if let Some(uid) = query.user_id {
        params.push(format!("user_id={uid}"));
    }
    if let Some(ref rt) = query.resource_type {
        if !rt.is_empty() {
            params.push(format!("resource_type={rt}"));
        }
    }
    if let Some(ref rid) = query.resource_id {
        if !rid.is_empty() {
            params.push(format!("resource_id={rid}"));
        }
    }
    let url = format!("/audit/rows?{}", params.join("&"));

    html! {
        div class="text-center py-4" {
            button type="button"
                   hx-get=(url)
                   hx-target="#audit-rows"
                   hx-swap="beforeend"
                   class="text-sm font-semibold text-slate-600 hover:text-slate-800 border border-slate-300 px-4 py-2 rounded transition" {
                "Load more"
            }
        }
    }
}
