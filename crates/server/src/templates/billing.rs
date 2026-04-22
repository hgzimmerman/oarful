//! Billing-related templates (trial expired, suspended).

use maud::{html, Markup};

use crate::tenant_cache::TenantConfig;
use lineup_db::app_user::Role;
use lineup_master_db::tenant::BillingStatus;

pub(crate) fn suspended_page(
    tenant_name: &str,
    config: &TenantConfig,
    webmaster_email: &str,
) -> Markup {
    let (title, message) = match config.billing_status {
        BillingStatus::Trial => (
            "Your free trial has expired",
            "Your 30-day trial has ended. Contact us to continue using Oarful.",
        ),
        BillingStatus::Suspended => (
            "Subscription suspended",
            "Your subscription has been suspended. Contact us to reactivate.",
        ),
        BillingStatus::Cancelled => (
            "Subscription cancelled",
            "Your subscription has been cancelled. Contact us if you'd like to return.",
        ),
        BillingStatus::Active | BillingStatus::Grandfathered => (
            "Account issue",
            "There's an issue with your account. Please contact support.",
        ),
    };

    super::layout::page(
        title,
        html! {
            div class="max-w-lg mx-auto mt-16 text-center" {
                div class="bg-white rounded-lg shadow p-8" {
                    div class="text-4xl mb-4" { "\u{26f5}" }
                    h2 class="text-xl font-bold text-slate-800 mb-2" { (title) }
                    p class="text-slate-600 mb-1" { (tenant_name) }
                    p class="text-sm text-slate-500 mb-6" { (message) }

                    @if let Some(exp) = config.trial_expires_at {
                        p class="text-xs text-slate-400 mb-6" {
                            "Trial expired: " (exp.format("%B %-d, %Y"))
                        }
                    }

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
