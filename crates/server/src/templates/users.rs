//! User management + invite templates.

use std::collections::HashMap;

use lineup_db::app_user::{AppUser, Role, UserId, UserStatus};
use lineup_db::rower::types::RowerId;
use maud::{html, Markup, DOCTYPE};

/// User list with invite form (PD-only page).
pub(crate) fn list_content(
    users: &[AppUser],
    roles: &HashMap<UserId, Role>,
    user_rower_map: &HashMap<UserId, RowerId>,
) -> Markup {
    let th_class =
        "px-4 py-2.5 text-left font-mono-stat text-[10px] tracking-widest uppercase font-semibold";
    html! {
        // ── Header ──
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" {
                "Users"
            }
            p class="font-mono-stat text-xs tracking-wide mt-1" style="color: var(--muted)" {
                (users.len()) " users"
            }
        }

        div class="px-4 sm:px-8 py-6 max-w-4xl mx-auto space-y-6" {
            // Invite form
            section class="rounded-lg p-6" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                h2 class="font-serif-heading text-lg font-medium tracking-tight mb-4" style="color: var(--ink)" { "Invite a new user" }
                form method="post" action="/users/invite"
                     class="grid grid-cols-1 md:grid-cols-5 gap-3 items-end" {
                    div {
                        label for="invite_first_name" class="block font-mono-stat text-[9px] tracking-widest uppercase font-semibold mb-1" style="color: var(--muted)" { "First name" }
                        input id="invite_first_name" name="first_name" type="text"
                              autocomplete="given-name"
                              class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                    }
                    div {
                        label for="invite_last_name" class="block font-mono-stat text-[9px] tracking-widest uppercase font-semibold mb-1" style="color: var(--muted)" { "Last name" }
                        input id="invite_last_name" name="last_name" type="text"
                              autocomplete="family-name"
                              class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                    }
                    div {
                        label for="invite_email" class="block font-mono-stat text-[9px] tracking-widest uppercase font-semibold mb-1" style="color: var(--muted)" { "Email" }
                        input id="invite_email" name="email" type="email" required
                              autocomplete="email"
                              class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                    }
                    div {
                        label for="invite_role" class="block font-mono-stat text-[9px] tracking-widest uppercase font-semibold mb-1" style="color: var(--muted)" { "Role" }
                        select id="invite_role" name="role"
                               class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none" {
                            option value="Member" { "Member" }
                            option value="Coach" { "Coach" }
                            option value="ProgramDirector" { "Program Director" }
                        }
                    }
                    button type="submit" class="btn-warm-ink py-2 px-4 text-sm" {
                        "Send invite"
                    }
                }
            }

            // User table
            @if !users.is_empty() {
                div class="rounded-lg overflow-x-auto" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                    table class="w-full text-sm min-w-[480px]" {
                        caption class="sr-only" { "Users" }
                        thead {
                            tr style="background: var(--paper-2)" {
                                th scope="col" class=(th_class) style="color: var(--ink-2)" { "Name" }
                                th scope="col" class=(th_class) style="color: var(--ink-2)" { "Email" }
                                th scope="col" class=(th_class) style="color: var(--ink-2)" { "Role" }
                                th scope="col" class=(th_class) style="color: var(--ink-2)" { "Status" }
                                th scope="col" class=(th_class) {}
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

    let status_style = match u.status {
        UserStatus::Active => "color: var(--good); background: color-mix(in oklch, var(--good) 10%, var(--paper)); border-color: color-mix(in oklch, var(--good) 22%, var(--rule))",
        UserStatus::Invited => "color: var(--warn); background: color-mix(in oklch, var(--warn) 10%, var(--paper)); border-color: color-mix(in oklch, var(--warn) 22%, var(--rule))",
        UserStatus::Disabled => "color: var(--ink-3); background: var(--paper-2); border-color: var(--rule)",
    };

    html! {
        tr id={"user-" (u.id)} style="border-top: 1px solid var(--rule-2)" class="hover:bg-paper-2" {
            td class="px-4 py-2.5" {
                @if let Some(rid) = rower_id {
                    a href={"/rowers/" (rid)}
                      hx-get={"/rowers/" (rid)}
                      hx-target="#content"
                      hx-swap="innerHTML transition:true"
                      hx-push-url="true"
                      class="font-serif-heading font-medium text-[15px] tracking-tight hover:underline" style="color: var(--link)" {
                        (u.display_name())
                    }
                } @else {
                    span class="font-serif-heading font-medium text-[15px] tracking-tight" style="color: var(--ink)" {
                        (u.display_name())
                    }
                }
            }
            td class="px-4 py-2.5" {
                span class="font-mono-stat text-xs" style="color: var(--muted)" { (u.email) }
            }
            td class="px-4 py-2.5" {
                span class="font-mono-stat text-xs" style="color: var(--ink-2)" { (role_label) }
            }
            td class="px-4 py-2.5" {
                span class="stat-badge text-[10px]" style=(status_style) { (u.status) }
            }
            td class="px-4 py-2.5 text-right space-x-2" {
                @if u.status == UserStatus::Invited {
                    form method="post"
                         action={"/users/" (u.id) "/resend-invite"}
                         hx-post={"/users/" (u.id) "/resend-invite"}
                         hx-target={"#user-" (u.id)}
                         hx-swap="outerHTML"
                         class="inline" {
                        button type="submit"
                               class="font-mono-stat text-[10px] font-semibold hover:underline" style="color: var(--link)" {
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
                               class="font-mono-stat text-[10px] font-semibold hover:underline" style="color: var(--muted)" {
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
                               class="font-mono-stat text-[10px] font-semibold hover:underline" style="color: var(--good)" {
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
        header class="border-b px-4 sm:px-8 py-3 sm:py-4" style="border-color: var(--rule); background: var(--paper)" {
            div class="flex items-center gap-3 mb-1" {
                a href="/users"
                  class="font-mono-stat text-xs tracking-wider hover:underline" style="color: var(--muted)" {
                    "← All users"
                }
            }
            h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" { "Invite" }
        }
        div class="px-4 sm:px-8 py-6 max-w-2xl mx-auto" {
            @if let Some(msg) = error {
                div class="bg-bad/10 border-l-4 border-bad px-4 py-3 rounded text-sm text-ink mb-4" {
                    strong { "Error. " } (msg)
                }
            }
            @if let Some(url) = invite_url {
                div class="rounded-lg px-4 py-3 text-sm mb-4" style="background: color-mix(in oklch, var(--good) 8%, var(--paper)); border-left: 4px solid var(--good); color: var(--ink)" {
                    p class="font-semibold mb-2" { "Invite created." }
                    p { "Share this link with the user:" }
                    code class="block mt-2 rounded px-3 py-2 font-mono text-xs break-all" style="background: var(--paper-2); border: 1px solid var(--rule)" {
                        (url)
                    }
                    p class="text-xs mt-2" style="color: var(--muted)" {
                        "The link expires in 7 days."
                    }
                }
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
                (super::layout::theme_init_script())
            }
            body class="bg-paper text-ink min-h-screen flex items-center justify-center" {
                div class="w-full max-w-sm" {
                    div class="text-center mb-8" {
                        h1 class="font-serif-heading text-2xl font-medium tracking-tight" style="color: var(--ink)" { "Oarful" }
                        p class="font-mono-stat text-xs tracking-wide mt-1" style="color: var(--muted)" { "Set your password to activate your account" }
                    }

                    @if let Some(msg) = error {
                        div class="mb-4 bg-bad/10 border-l-4 border-bad px-4 py-3 rounded text-sm text-ink" {
                            (msg)
                        }
                    }

                    form method="post" action=(action)
                         class="rounded-lg p-6 space-y-4" style="background: var(--paper); box-shadow: var(--shadow-soft)" {
                        div {
                            label for="password" class="block font-mono-stat text-[10px] tracking-widest uppercase font-semibold mb-1.5" style="color: var(--ink-2)" { "Password" }
                            input id="password" name="password" type="password" required minlength="8"
                                  autocomplete="new-password"
                                  class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                        }
                        div {
                            label for="password_confirm" class="block font-mono-stat text-[10px] tracking-widest uppercase font-semibold mb-1.5" style="color: var(--ink-2)" { "Confirm password" }
                            input id="password_confirm" name="password_confirm" type="password" required
                                  autocomplete="new-password"
                                  class="w-full border border-rule rounded px-3 py-2 text-sm focus:border-ink-3 focus:outline-none";
                        }
                        button type="submit" class="w-full btn-warm-ink py-2" {
                            "Activate account"
                        }
                    }
                }
            }
        }
    }
}
