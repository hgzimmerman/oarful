//! Superuser admin panel templates.

use lineup_master_db::tenant::{BillingStatus, Tenant};
use maud::{html, Markup, DOCTYPE};

/// Page shell for the superuser panel (no tenant navbar).
fn su_shell(title: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " · Oarful Admin" }
                link rel="stylesheet" href="/tailwind.css";
                script src="/htmx.min.js" {}
            }
            body class="bg-slate-50 text-slate-900 min-h-screen flex flex-col" {
                nav class="bg-slate-800 text-white px-6 py-3 flex items-center justify-between" {
                    span class="font-bold text-lg" { "Oarful Admin" }
                    form method="post" action="/logout" class="inline" {
                        button type="submit"
                               class="px-3 py-2 rounded hover:bg-white/10 transition text-sm" {
                            "Logout"
                        }
                    }
                }
                main class="flex-grow p-4 sm:p-8" {
                    (content)
                }
            }
        }
    }
}

/// Main dashboard: tenant list with billing controls and impersonation.
pub(crate) fn su_dashboard(tenants: &[Tenant]) -> Markup {
    su_shell(
        "Tenants",
        html! {
            h1 class="text-2xl font-bold text-slate-800 mb-6" { "Tenants" }

            // Create tenant form
            details class="mb-8 bg-white rounded-lg shadow" {
                summary class="px-4 py-3 cursor-pointer text-sm font-semibold text-slate-700 hover:text-slate-900" {
                    "Create grandfathered tenant"
                }
                form method="post" action="/su/create-tenant"
                     class="px-4 pb-4 grid grid-cols-1 sm:grid-cols-2 gap-4" {
                    div {
                        label for="club_name" class="block text-sm font-medium text-slate-700 mb-1" { "Club name" }
                        input id="club_name" name="club_name" type="text" required
                              class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                    }
                    div {
                        label for="admin_name" class="block text-sm font-medium text-slate-700 mb-1" { "Admin name" }
                        input id="admin_name" name="admin_name" type="text" required
                              class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                    }
                    div {
                        label for="admin_email" class="block text-sm font-medium text-slate-700 mb-1" { "Admin email" }
                        input id="admin_email" name="admin_email" type="email" required
                              class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                    }
                    div {
                        label for="admin_password" class="block text-sm font-medium text-slate-700 mb-1" { "Password" }
                        input id="admin_password" name="admin_password" type="password" required minlength="8"
                              class="w-full border border-slate-300 rounded px-3 py-2 text-sm focus:border-slate-500 focus:outline-none";
                    }
                    div class="sm:col-span-2" {
                        button type="submit"
                               class="bg-slate-800 hover:bg-slate-900 text-white font-semibold text-sm px-4 py-2 rounded shadow transition" {
                            "Create tenant"
                        }
                    }
                }
            }

            div class="bg-white rounded-lg shadow overflow-x-auto" {
                table class="w-full text-sm" {
                    thead class="bg-slate-50 border-b border-slate-200" {
                        tr {
                            th class="text-left px-4 py-3 font-semibold text-slate-600" { "Name" }
                            th class="text-left px-4 py-3 font-semibold text-slate-600" { "Slug" }
                            th class="text-left px-4 py-3 font-semibold text-slate-600" { "Status" }
                            th class="text-left px-4 py-3 font-semibold text-slate-600" { "Trial Expires" }
                            th class="text-left px-4 py-3 font-semibold text-slate-600" { "Created" }
                            th class="text-left px-4 py-3 font-semibold text-slate-600" { "Actions" }
                        }
                    }
                    tbody {
                        @for tenant in tenants {
                            (su_tenant_row(tenant))
                        }
                    }
                }
            }
        },
    )
}

/// Single tenant row — used for initial render and HTMX swap after
/// billing update.
pub(crate) fn su_tenant_row(tenant: &Tenant) -> Markup {
    let status = tenant.billing_status;
    let row_id = format!("tenant-{}", tenant.id);
    let is_demo = tenant.is_demo();

    html! {
        tr id=(row_id) class="border-b border-slate-100 hover:bg-slate-50" {
            td class="px-4 py-3" {
                (tenant.name)
                @if is_demo {
                    span class="ml-1 text-xs bg-amber-100 text-amber-700 px-1.5 py-0.5 rounded" {
                        "demo"
                    }
                }
            }
            td class="px-4 py-3 font-mono text-xs text-slate-500" { (tenant.slug) }
            td class="px-4 py-3" {
                form hx-post=(format!("/su/billing/{}", tenant.id))
                     hx-target=(format!("#{row_id}"))
                     hx-swap="outerHTML"
                     class="flex items-center gap-2" {
                    select name="status"
                           class="border border-slate-300 rounded px-2 py-1 text-sm" {
                        @for s in &[BillingStatus::Trial, BillingStatus::Active, BillingStatus::Grandfathered, BillingStatus::Suspended, BillingStatus::Cancelled] {
                            option value=(s.as_str()) selected[*s == status] {
                                (s.as_str())
                            }
                        }
                    }
                    button type="submit"
                           class="text-xs bg-slate-700 text-white px-2 py-1 rounded hover:bg-slate-800 transition" {
                        "Save"
                    }
                }
            }
            td class="px-4 py-3 text-xs text-slate-500" {
                @if let Some(exp) = tenant.trial_expires_at {
                    (exp.format("%Y-%m-%d"))
                } @else {
                    "—"
                }
            }
            td class="px-4 py-3 text-xs text-slate-500" {
                (tenant.created_at.format("%Y-%m-%d"))
            }
            td class="px-4 py-3" {
                @if !is_demo {
                    form method="post" action=(format!("/su/impersonate/{}", tenant.id))
                         class="inline" {
                        button type="submit"
                               class="text-xs border border-slate-300 hover:border-slate-400 text-slate-600 hover:text-slate-800 px-3 py-1 rounded transition" {
                            "Impersonate"
                        }
                    }
                }
            }
        }
    }
}
