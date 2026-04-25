//! Stripe billing handlers: checkout, success page, status poll, portal.
//!
//! All handlers extract `State<Option<StripeCtx>>` and return 404 when
//! Stripe is unconfigured. This module is always compiled but the routes
//! are only mounted when `stripe_ctx` is `Some`.

use axum::{
    extract::State,
    response::{Html, Redirect},
    Extension,
};
use lineup_db::app_user::Role;
use lineup_master_db::tenant::{BillingStatus, Tenant, TenantId};
use stripe_billing::billing_portal_session::CreateBillingPortalSession;
use stripe_checkout::checkout_session::{CreateCheckoutSession, CreateCheckoutSessionLineItems};
use stripe_checkout::CheckoutSessionMode;
use stripe_core::customer::CreateCustomer;

use crate::state::{AppState, StripeCtx, TenantContext};

use super::{internal_error, not_found, ErrorResponse};

/// `POST /billing/checkout` — create a Stripe Checkout Session and
/// redirect to Stripe's hosted payment page. PD+ only.
#[tracing::instrument(level = "info", skip_all, err)]
pub(crate) async fn checkout_handler(
    State(state): State<AppState>,
    State(stripe_ctx): State<Option<StripeCtx>>,
    Extension(tenant): Extension<TenantContext>,
) -> Result<Redirect, ErrorResponse> {
    let stripe = stripe_ctx.ok_or_else(|| not_found("Billing is not configured."))?;
    let role = tenant.claims.role();
    if !role.at_least(Role::ProgramDirector) {
        return Err(ErrorResponse(
            axum::http::StatusCode::FORBIDDEN,
            "Only a program director can manage billing.".into(),
        ));
    }

    let tenant_id = tenant.tenant_id;
    let tenant_name = tenant.config.tenant_name.clone();

    // Look up or create the Stripe customer for this tenant.
    let customer_id = get_or_create_customer(&state, &stripe, tenant_id, &tenant_name).await?;

    // Build checkout session.
    let origin = state
        .origin
        .as_ref()
        .map(|u| u.to_string())
        .unwrap_or_default();

    let mut line_item = CreateCheckoutSessionLineItems::new();
    line_item.price = Some(stripe.price_id.0.clone());
    line_item.quantity = Some(1);

    let session = CreateCheckoutSession::new()
        .customer(customer_id)
        .mode(CheckoutSessionMode::Subscription)
        .line_items(vec![line_item])
        .success_url(format!("{origin}/billing/success"))
        .cancel_url(format!("{origin}/admin/settings"))
        .client_reference_id(tenant_id.to_string())
        .send(&stripe.client)
        .await
        .map_err(|e| {
            tracing::error!(?e, "Stripe checkout session creation failed");
            internal_error(e)
        })?;

    let url = session.url.ok_or_else(|| {
        tracing::error!("Stripe checkout session missing URL");
        internal_error("checkout session has no URL")
    })?;

    Ok(Redirect::to(&url))
}

/// `GET /billing/success` — landing page after Stripe Checkout.
/// Polls `/billing/status` via HTMX until billing flips to Active.
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) async fn success_handler(Extension(tenant): Extension<TenantContext>) -> Html<String> {
    use maud::{html, DOCTYPE};

    let is_active = matches!(
        tenant.config.billing_status,
        BillingStatus::Active | BillingStatus::Grandfathered
    );

    let markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Payment — Oarful" }
                link rel="stylesheet" href="/tailwind.css";
                script src="/htmx.min.js" defer {}
            }
            body class="bg-slate-50 text-slate-900 min-h-screen flex items-center justify-center" {
                div id="billing-status" class="text-center max-w-md px-6" {
                    @if is_active {
                        (active_fragment())
                    } @else {
                        (pending_fragment())
                    }
                }
            }
        }
    };
    Html(markup.into_string())
}

/// `GET /billing/status` — tiny fragment for HTMX polling.
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) async fn status_handler(Extension(tenant): Extension<TenantContext>) -> Html<String> {
    let is_active = matches!(
        tenant.config.billing_status,
        BillingStatus::Active | BillingStatus::Grandfathered
    );
    let markup = if is_active {
        active_fragment()
    } else {
        pending_fragment()
    };
    Html(markup.into_string())
}

/// `GET /billing/portal` — redirect to Stripe's customer portal. PD+ only.
#[tracing::instrument(level = "info", skip_all, err)]
pub(crate) async fn portal_handler(
    State(state): State<AppState>,
    State(stripe_ctx): State<Option<StripeCtx>>,
    Extension(tenant): Extension<TenantContext>,
) -> Result<Redirect, ErrorResponse> {
    let stripe = stripe_ctx.ok_or_else(|| not_found("Billing is not configured."))?;
    let role = tenant.claims.role();
    if !role.at_least(Role::ProgramDirector) {
        return Err(ErrorResponse(
            axum::http::StatusCode::FORBIDDEN,
            "Only a program director can manage billing.".into(),
        ));
    }

    let tenant_id = tenant.tenant_id;

    // Look up the Stripe customer ID from the tenant record.
    let tenant_row = state
        .master_db
        .with_conn(move |conn| Tenant::get(conn, tenant_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("Tenant not found."))?;

    let customer_id = tenant_row
        .stripe_customer_id
        .ok_or_else(|| not_found("No Stripe customer linked to this club."))?;

    let origin = state
        .origin
        .as_ref()
        .map(|u| u.to_string())
        .unwrap_or_default();

    let session = CreateBillingPortalSession::new()
        .customer(customer_id)
        .return_url(format!("{origin}/admin/settings"))
        .send(&stripe.client)
        .await
        .map_err(|e| {
            tracing::error!(?e, "Stripe portal session creation failed");
            internal_error(e)
        })?;

    Ok(Redirect::to(&session.url))
}

// ── Helpers ─────────────────────────────────────────────────────

/// Ensure a Stripe customer exists for this tenant. Creates one if
/// the tenant has no `stripe_customer_id` yet.
async fn get_or_create_customer(
    state: &AppState,
    stripe: &StripeCtx,
    tenant_id: TenantId,
    tenant_name: &str,
) -> Result<String, ErrorResponse> {
    // Check if we already have a Stripe customer.
    let existing = state
        .master_db
        .with_conn(move |conn| Tenant::get(conn, tenant_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("Tenant not found."))?;

    if let Some(cid) = existing.stripe_customer_id {
        return Ok(cid);
    }

    // Create a new Stripe customer.
    let name = tenant_name.to_string();
    let customer = CreateCustomer::new()
        .name(name)
        .send(&stripe.client)
        .await
        .map_err(|e| {
            tracing::error!(?e, "Stripe customer creation failed");
            internal_error(e)
        })?;

    let customer_id = customer.id.as_str().to_string();

    // Store it on the tenant.
    let cid = customer_id.clone();
    state
        .master_db
        .with_conn(move |conn| Tenant::set_stripe_ids(conn, tenant_id, &cid, None))
        .await
        .map_err(internal_error)?;

    Ok(customer_id)
}

fn active_fragment() -> maud::Markup {
    maud::html! {
        h1 class="text-2xl font-bold text-slate-800 mb-4" {
            "You're all set"
        }
        p class="text-slate-600 mb-6" {
            "Email sending is now enabled for your club."
        }
        a href="/admin/settings"
          class="inline-block bg-slate-800 text-white font-semibold px-6 py-3 rounded-lg hover:bg-slate-900 transition text-sm" {
            "Back to settings"
        }
    }
}

fn pending_fragment() -> maud::Markup {
    maud::html! {
        h1 class="text-2xl font-bold text-slate-800 mb-4" {
            "Payment received"
        }
        p class="text-slate-600 mb-6" {
            "Activating your account…"
        }
        div class="flex justify-center mb-6" {
            div class="animate-spin rounded-full h-8 w-8 border-b-2 border-slate-800" {}
        }
        div hx-get="/billing/status"
             hx-trigger="every 2s"
             hx-target="#billing-status"
             hx-swap="innerHTML" {}
    }
}
