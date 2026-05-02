//! User management + invite templates.

use std::collections::HashMap;

use lineup_db::app_user::{AppUser, Role, UserId, UserStatus};
use lineup_db::rower::types::RowerId;
use maud::{html, Markup, DOCTYPE};

use super::layout::page_header;

/// User list with invite form (PD-only page).
pub(crate) fn list_content(
    users: &[AppUser],
    roles: &HashMap<UserId, Role>,
    user_rower_map: &HashMap<UserId, RowerId>,
) -> Markup {
    let subtitle = format!("{} users", users.len());
    html! {
        (page_header("Users", Some(&subtitle)))
        div class="px-4 sm:px-8 py-6 max-w-4xl mx-auto space-y-6" {
            // Invite form
            section class="bg-paper rounded-lg shadow-soft p-6" {
                h2 class="text-lg font-bold text-ink mb-4" { "Invite a new user" }
                form method="post" action="/users/invite"
                     class="grid grid-cols-1 md:grid-cols-4 gap-3 items-end" {
                    div {
                        label for="invite_name" class="block text-xs font-semibold text-ink-2 uppercase tracking-wide mb-1" { "Name" }
                        input id="invite_name" name="name" type="text" required
                              class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                    }
                    div {
                        label for="invite_email" class="block text-xs font-semibold text-ink-2 uppercase tracking-wide mb-1" { "Email" }
                        input id="invite_email" name="email" type="email" required
                              class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                    }
                    div {
                        label for="invite_role" class="block text-xs font-semibold text-ink-2 uppercase tracking-wide mb-1" { "Role" }
                        select id="invite_role" name="role"
                               class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none" {
                            option value="Member" { "Member" }
                            option value="Coach" { "Coach" }
                            option value="ProgramDirector" { "Program Director" }
                        }
                    }
                    button type="submit"
                           class="bg-good hover:opacity-90 text-paper text-sm font-semibold px-4 py-2 rounded shadow-soft transition" {
                        "Send invite"
                    }
                }
            }

            // User table
            @if !users.is_empty() {
                div class="bg-paper rounded-lg shadow-soft overflow-x-auto" {
                    table class="w-full text-sm min-w-[480px]" {
                        caption class="sr-only" { "Users" }
                        thead class="bg-paper-2 text-left text-xs uppercase text-ink-2" {
                            tr {
                                th scope="col" class="px-4 py-2" { "Name" }
                                th scope="col" class="px-4 py-2" { "Email" }
                                th scope="col" class="px-4 py-2" { "Role" }
                                th scope="col" class="px-4 py-2" { "Status" }
                                th scope="col" class="px-4 py-2" {}
                            }
                        }
                        tbody {
                            @for u in users {
                                (user_row(u, roles, user_rower_map))
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn user_row(
    u: &AppUser,
    roles: &HashMap<UserId, Role>,
    user_rower_map: &HashMap<UserId, RowerId>,
) -> Markup {
    let role_label = roles
        .get(&u.id)
        .map(|r| match r {
            Role::Member => "Member",
            Role::Coach => "Coach",
            Role::ProgramDirector => "Program Director",
        })
        .unwrap_or("—");
    let rower_id = user_rower_map.get(&u.id);
    html! {
        tr id={"user-" (u.id)} class="border-t border-rule-2" {
            td class="px-4 py-2 font-medium text-ink" {
                @if let Some(rid) = rower_id {
                    a href={"/rowers/" (rid)}
                      hx-get={"/rowers/" (rid)}
                      hx-target="#content"
                      hx-push-url="true"
                      class="text-link hover:text-link-2 hover:underline" {
                        (u.name)
                    }
                } @else {
                    (u.name)
                }
            }
            td class="px-4 py-2 text-ink-2" { (u.email) }
            td class="px-4 py-2 text-ink-2" { (role_label) }
            td class="px-4 py-2" {
                @let badge_class = match u.status {
                    UserStatus::Active => "bg-good/15 text-good",
                    UserStatus::Invited => "bg-warn/15 text-warn",
                    UserStatus::Disabled => "bg-paper-3 text-ink-2",
                };
                span class={"text-xs px-2 py-0.5 rounded-full " (badge_class)} {
                    (u.status)
                }
            }
            td class="px-4 py-2 text-right space-x-2" {
                @if u.status == UserStatus::Invited {
                    form method="post"
                         action={"/users/" (u.id) "/resend-invite"}
                         hx-post={"/users/" (u.id) "/resend-invite"}
                         hx-target={"#user-" (u.id)}
                         hx-swap="outerHTML"
                         class="inline" {
                        button type="submit"
                               class="text-xs text-link hover:text-link-2 font-medium" {
                            "Resend invite"
                        }
                    }
                }
                @if u.status == UserStatus::Active {
                    form method="post"
                         action={"/users/" (u.id) "/toggle-status"}
                         hx-post={"/users/" (u.id) "/toggle-status"}
                         hx-target={"#user-" (u.id)}
                         hx-swap="outerHTML"
                         class="inline" {
                        button type="submit"
                               class="text-xs text-muted hover:text-red-600 font-medium" {
                            "Disable"
                        }
                    }
                }
                @if u.status == UserStatus::Disabled {
                    form method="post"
                         action={"/users/" (u.id) "/toggle-status"}
                         hx-post={"/users/" (u.id) "/toggle-status"}
                         hx-target={"#user-" (u.id)}
                         hx-swap="outerHTML"
                         class="inline" {
                        button type="submit"
                               class="text-xs text-emerald-600 hover:text-emerald-800 font-medium" {
                            "Enable"
                        }
                    }
                }
            }
        }
    }
}

/// Result page after creating an invite.
pub(crate) fn invite_result(invite_url: Option<&str>, error: Option<&str>) -> Markup {
    html! {
        (page_header("Invite", None))
        div class="px-4 sm:px-8 py-6 max-w-2xl mx-auto" {
            @if let Some(msg) = error {
                div class="bg-bad/10 border-l-4 border-bad px-4 py-3 rounded text-sm text-ink mb-4" {
                    strong { "Error. " } (msg)
                }
            }
            @if let Some(url) = invite_url {
                div class="bg-emerald-50 border-l-4 border-emerald-500 px-4 py-3 rounded text-sm text-emerald-900 mb-4" {
                    p class="font-semibold mb-2" { "Invite created." }
                    p { "Share this link with the user:" }
                    code class="block mt-2 bg-paper px-3 py-2 rounded border border-emerald-200 font-mono text-xs break-all" {
                        (url)
                    }
                    p class="text-xs mt-2 text-emerald-700" {
                        "The link expires in 7 days."
                    }
                }
            }
            a href="/users"
              class="text-emerald-700 hover:text-emerald-900 font-semibold text-sm" {
                "← Back to users"
            }
        }
    }
}

/// Standalone password-set form for accepting an invite (no navbar —
/// user isn't authenticated yet).
pub(crate) fn accept_form(action: &str, error: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Set password · Oarful" }
                script src="/tailwindcss.js" {}
            }
            body class="bg-paper text-ink min-h-screen flex items-center justify-center" {
                div class="w-full max-w-sm" {
                    div class="text-center mb-8" {
                        h1 class="text-2xl font-bold text-ink" { "Oarful" }
                        p class="text-sm text-ink-3 mt-1" { "Set your password to activate your account" }
                    }

                    @if let Some(msg) = error {
                        div class="mb-4 bg-bad/10 border-l-4 border-bad px-4 py-3 rounded text-sm text-ink" {
                            (msg)
                        }
                    }

                    form method="post" action=(action)
                         class="bg-paper rounded-lg shadow-soft p-6 space-y-4" {
                        div {
                            label for="password" class="block text-sm font-semibold text-ink-2 mb-1" { "Password" }
                            input id="password" name="password" type="password" required minlength="8"
                                  class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                        }
                        div {
                            label for="password_confirm" class="block text-sm font-semibold text-ink-2 mb-1" { "Confirm password" }
                            input id="password_confirm" name="password_confirm" type="password" required
                                  class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                        }
                        button type="submit"
                               class="w-full bg-ink hover:bg-ink-2 text-paper font-semibold py-2 rounded shadow-soft transition" {
                            "Activate account"
                        }
                    }
                }
            }
        }
    }
}
