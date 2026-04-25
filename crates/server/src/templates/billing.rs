//! Billing-related templates (demo expired).

use maud::{html, Markup};

use lineup_db::app_user::Role;

/// Shown when a demo tenant has expired.
pub(crate) fn suspended_page(tenant_name: &str, webmaster_email: &str) -> Markup {
    let title = "Demo expired";
    let message = "This demo has expired. Contact us or sign up for a new account.";

    super::layout::page(
        title,
        html! {
            div class="max-w-lg mx-auto mt-16 text-center" {
                div class="bg-white rounded-lg shadow p-8" {
                    div class="text-4xl mb-4" { "\u{26f5}" }
                    h2 class="text-xl font-bold text-slate-800 mb-2" { (title) }
                    p class="text-slate-600 mb-1" { (tenant_name) }
                    p class="text-sm text-slate-500 mb-6" { (message) }

                    div class="space-y-3" {
                        a href={"mailto:" (webmaster_email)}
                          class="block w-full bg-slate-800 hover:bg-slate-900 text-white font-semibold py-2 rounded shadow transition text-sm" {
                            "Contact us"
                        }
                        form method="post" action="/logout" {
                            button type="submit"
                                   class="w-full border border-slate-300 hover:border-slate-400 text-slate-600 hover:text-slate-800 font-medium py-2 rounded transition text-sm" {
                                "Sign out"
                            }
                        }
                    }
                }
            }
        },
        Role::Member, // minimal navbar for billing page
        false,
    )
}
