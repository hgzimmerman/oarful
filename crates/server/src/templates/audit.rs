//! Audit log list view with filters and "load more" pagination.

use std::collections::HashMap;

use lineup_db::app_user::UserId;
use lineup_db::audit_log::AuditLog;
use lineup_db::types::AuditAction;
use maud::{html, Markup};

use crate::handlers::audit::{AuditQuery, PAGE_SIZE};

pub(crate) fn list_content(
    entries: &[AuditLog],
    actions: &[AuditAction],
    user_map: &HashMap<UserId, String>,
    query: &AuditQuery,
    offset: i64,
    has_more: bool,
) -> Markup {
    html! {
        header class="bg-paper border-b border-rule-2 px-4 sm:px-8 py-4 sm:py-6" {
            h1 class="text-2xl font-bold text-ink" { "Audit log" }
            p class="text-sm text-ink-3 mt-1" { "90-day history of changes" }
        }

        div class="px-4 sm:px-8 py-6 space-y-4 max-w-6xl mx-auto" {
            // Filters
            (filter_bar(actions, user_map, query))

            // Table
            @if entries.is_empty() && offset == 0 {
                div class="text-center text-ink-3 italic py-12" {
                    "No audit entries found."
                }
            } @else {
                div class="bg-paper rounded-lg shadow-soft overflow-hidden" {
                    table class="w-full text-sm" {
                        caption class="sr-only" { "Audit log" }
                        thead class="bg-paper-2 text-left text-xs uppercase text-ink-2" {
                            tr {
                                th scope="col" class="px-4 py-2" { "When" }
                                th scope="col" class="px-4 py-2" { "User" }
                                th scope="col" class="px-4 py-2" { "Action" }
                                th scope="col" class="px-4 py-2" { "Resource" }
                                th scope="col" class="px-4 py-2" { "Detail" }
                            }
                        }
                        tbody #audit-rows "aria-live"="polite" {
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
            tr class="border-t border-rule-2 hover:bg-paper-2" {
                td class="px-4 py-2 text-xs text-ink-3 whitespace-nowrap" {
                    (entry.timestamp.format("%Y-%m-%d %H:%M"))
                }
                td class="px-4 py-2" {
                    @if let Some(uid) = entry.user_id {
                        @if let Some(name) = user_map.get(&uid) {
                            span class="text-ink" { (name) }
                        } @else {
                            span class="text-muted" { "user #" (uid) }
                        }
                    } @else {
                        span class="text-muted italic" { "system" }
                    }
                }
                td class="px-4 py-2" {
                    (action_badge(entry.action.as_str()))
                }
                td class="px-4 py-2 text-xs font-mono text-ink-2" {
                    (entry.resource_type) "/" (entry.resource_id)
                }
                td class="px-4 py-2 text-xs text-ink-3 max-w-xs truncate" {
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
        "lineup" => ("bg-blue-100/80", "text-blue-700"),
        "rower" => ("bg-emerald-100/80", "text-emerald-700"),
        "boat" => ("bg-amber-100/80", "text-amber-700"),
        "practice" => ("bg-violet-100/80", "text-violet-700"),
        "invite" => ("bg-pink-100/80", "text-pink-700"),
        "sync" => ("bg-cyan-100/80", "text-cyan-700"),
        "solver_profile" => ("bg-orange-100/80", "text-orange-700"),
        "team" => ("bg-indigo-100/80", "text-indigo-700"),
        "availability" => ("bg-teal-100/80", "text-teal-700"),
        _ => ("bg-paper-2", "text-ink"),
    };
    html! {
        span class=(format!("{bg} {text} text-xs px-1.5 py-0.5 rounded-full")) {
            (action)
        }
    }
}

fn filter_bar(
    actions: &[AuditAction],
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
                label class="block text-xs text-ink-3 mb-1" { "Action" }
                select name="action"
                       class="border border-rule rounded px-2 py-1.5 text-sm" {
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
                label class="block text-xs text-ink-3 mb-1" { "User" }
                select name="user_id"
                       class="border border-rule rounded px-2 py-1.5 text-sm" {
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
                label class="block text-xs text-ink-3 mb-1" { "Resource" }
                select name="resource_type"
                       class="border border-rule rounded px-2 py-1.5 text-sm" {
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
                   class="bg-ink-2 hover:bg-ink text-paper text-sm font-semibold px-4 py-1.5 rounded transition" {
                "Filter"
            }

            @if query.action.is_some() || query.user_id.is_some() || query.resource_type.is_some() {
                a href="/admin/audit"
                  hx-get="/admin/audit"
                  hx-target="#content"
                  hx-push-url="true"
                  class="text-sm text-ink-3 hover:text-ink" {
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
                   class="text-sm font-semibold text-ink-2 hover:text-ink border border-rule px-4 py-2 rounded transition" {
                "Load more"
            }
        }
    }
}
