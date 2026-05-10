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
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" { "Audit log" }
            p class="font-mono-stat text-xs tracking-wide mt-1" style="color: var(--muted)" { "90-day history of changes" }
        }

        div class="px-4 sm:px-8 py-6 space-y-4 max-w-6xl mx-auto" {
            // Filters
            (filter_bar(actions, user_map, query))

            // Table
            @if entries.is_empty() && offset == 0 {
                div class="text-center font-mono-stat text-xs italic py-12" style="color: var(--muted)" {
                    "No audit entries found."
                }
            } @else {
                div class="rounded-lg overflow-hidden" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                    table class="w-full text-sm" {
                        caption class="sr-only" { "Audit log" }
                        thead {
                            tr style="background: var(--paper-2)" {
                                th scope="col" class="px-4 py-2.5 text-left font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "When" }
                                th scope="col" class="px-4 py-2.5 text-left font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "User" }
                                th scope="col" class="px-4 py-2.5 text-left font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "Action" }
                                th scope="col" class="px-4 py-2.5 text-left font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "Resource" }
                                th scope="col" class="px-4 py-2.5 text-left font-mono-stat text-[10px] tracking-widest uppercase font-semibold" style="color: var(--ink-2)" { "Detail" }
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
            tr style="border-top: 1px solid var(--rule-2)" class="hover:bg-paper-2" {
                td class="px-4 py-2.5 whitespace-nowrap" {
                    span class="font-mono-stat text-[11px]" style="color: var(--muted)" {
                        (entry.timestamp.format("%Y-%m-%d %H:%M"))
                    }
                }
                td class="px-4 py-2.5" {
                    @if let Some(uid) = entry.user_id {
                        @if let Some(name) = user_map.get(&uid) {
                            span class="font-serif-heading font-medium text-[13px]" style="color: var(--ink)" { (name) }
                        } @else {
                            span class="font-mono-stat text-xs" style="color: var(--muted)" { "user #" (uid) }
                        }
                    } @else {
                        span class="font-mono-stat text-xs italic" style="color: var(--muted)" { "system" }
                    }
                }
                td class="px-4 py-2.5" {
                    (action_badge(entry.action.as_str()))
                }
                td class="px-4 py-2.5" {
                    span class="font-mono-stat text-[11px]" style="color: var(--ink-2)" {
                        (entry.resource_type) "/" (entry.resource_id)
                    }
                }
                td class="px-4 py-2.5 max-w-xs truncate" {
                    @if let Some(ref d) = entry.detail {
                        span class="font-mono-stat text-[11px]" style="color: var(--muted)" { (d) }
                    }
                }
            }
        }
    }
}

fn action_badge(action: &str) -> Markup {
    let style = match action.split('.').next().unwrap_or("") {
        "lineup" => "color: var(--link); background: color-mix(in oklch, var(--link) 10%, var(--paper)); border-color: color-mix(in oklch, var(--link) 22%, var(--rule))",
        "rower" => "color: var(--good); background: color-mix(in oklch, var(--good) 10%, var(--paper)); border-color: color-mix(in oklch, var(--good) 22%, var(--rule))",
        "boat" => "color: var(--warn); background: color-mix(in oklch, var(--warn) 10%, var(--paper)); border-color: color-mix(in oklch, var(--warn) 22%, var(--rule))",
        "practice" => "color: var(--cox); background: color-mix(in oklch, var(--cox) 10%, var(--paper)); border-color: color-mix(in oklch, var(--cox) 22%, var(--rule))",
        "invite" | "sync" => "color: var(--accent); background: color-mix(in oklch, var(--accent) 10%, var(--paper)); border-color: color-mix(in oklch, var(--accent) 22%, var(--rule))",
        "availability" => "color: var(--stbd); background: color-mix(in oklch, var(--stbd) 10%, var(--paper)); border-color: color-mix(in oklch, var(--stbd) 22%, var(--rule))",
        "team" | "solver_profile" => "color: var(--ink-3); background: var(--paper-2); border-color: var(--rule)",
        _ => "color: var(--ink-3); background: var(--paper-2); border-color: var(--rule)",
    };
    html! {
        span class="stat-badge text-[10px]" style=(style) { (action) }
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
             hx-swap="innerHTML transition:true"
             hx-push-url="true"
             class="flex flex-wrap items-end gap-3" {

            div {
                label class="block font-mono-stat text-[9px] tracking-widest uppercase font-semibold mb-1" style="color: var(--muted)" { "Action" }
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
                label class="block font-mono-stat text-[9px] tracking-widest uppercase font-semibold mb-1" style="color: var(--muted)" { "User" }
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
                label class="block font-mono-stat text-[9px] tracking-widest uppercase font-semibold mb-1" style="color: var(--muted)" { "Resource" }
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

            button type="submit" class="btn-warm-ink py-1.5 px-4 text-sm" {
                "Filter"
            }

            @if query.action.is_some() || query.user_id.is_some() || query.resource_type.is_some() {
                a href="/admin/audit"
                  hx-get="/admin/audit"
                  hx-target="#content"
                  hx-swap="innerHTML transition:true"
                  hx-push-url="true"
                  class="font-mono-stat text-xs hover:underline" style="color: var(--muted)" {
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
                   class="btn-warm-ghost text-xs py-2" {
                "Load more"
            }
        }
    }
}
