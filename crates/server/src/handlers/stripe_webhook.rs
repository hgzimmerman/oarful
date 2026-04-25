//! Stripe webhook handler. Verifies the signature, dispatches events,
//! and updates tenant billing status accordingly.

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use lineup_master_db::tenant::{BillingStatus, Tenant};
use stripe_webhook::{Event, EventObject, Webhook};

use crate::state::AppState;

/// `POST /stripe/webhook` — called by Stripe. No auth middleware — the
/// webhook signature serves as authentication.
#[tracing::instrument(level = "info", skip_all)]
pub(crate) async fn webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let stripe = match &state.stripe_ctx {
        Some(s) => s,
        None => return StatusCode::NOT_FOUND,
    };

    let payload = match std::str::from_utf8(&body) {
        Ok(p) => p,
        Err(_) => {
            tracing::warn!("webhook: invalid UTF-8 payload");
            return StatusCode::BAD_REQUEST;
        }
    };

    let sig = match headers
        .get("Stripe-Signature")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s,
        None => {
            tracing::warn!("webhook: missing Stripe-Signature header");
            return StatusCode::BAD_REQUEST;
        }
    };

    let event: Event = match Webhook::construct_event(payload, sig, &stripe.webhook_secret.0) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(?e, "webhook: signature verification failed");
            return StatusCode::BAD_REQUEST;
        }
    };

    let event_type = event.type_.as_str().to_string();
    tracing::info!(%event_type, event_id = %event.id.as_str(), "webhook received");

    match event.data.object {
        EventObject::CheckoutSessionCompleted(session) => {
            handle_checkout_completed(&state, &session).await
        }
        EventObject::CustomerSubscriptionDeleted(sub) => {
            handle_subscription_deleted(&state, &sub).await
        }
        EventObject::InvoicePaymentFailed(invoice) => handle_payment_failed(&state, &invoice).await,
        _ => {
            tracing::debug!(%event_type, "webhook: ignoring unhandled event type");
        }
    }

    StatusCode::OK
}

async fn handle_checkout_completed(state: &AppState, session: &stripe_shared::CheckoutSession) {
    let customer_id = match &session.customer {
        Some(expandable) => expandable.id().as_str().to_string(),
        None => {
            tracing::warn!("checkout.session.completed: no customer");
            return;
        }
    };

    let subscription_id = session
        .subscription
        .as_ref()
        .map(|s| s.id().as_str().to_string());

    let cid = customer_id.clone();
    let tenant = match state
        .master_db
        .with_conn(move |conn| Tenant::find_by_stripe_customer_id(conn, &cid))
        .await
    {
        Ok(Some(t)) => t,
        Ok(None) => {
            tracing::warn!(%customer_id, "checkout.session.completed: no tenant for Stripe customer");
            return;
        }
        Err(e) => {
            tracing::error!(?e, "checkout.session.completed: DB error");
            return;
        }
    };

    let tenant_id = tenant.id;
    let sub_id = subscription_id.clone();
    let cid2 = customer_id.clone();
    if let Err(e) = state
        .master_db
        .with_conn(move |conn| {
            Tenant::set_billing_status(conn, tenant_id, BillingStatus::Active)?;
            Tenant::set_stripe_ids(conn, tenant_id, &cid2, sub_id.as_deref())?;
            Ok::<_, diesel::result::Error>(())
        })
        .await
    {
        tracing::error!(?e, %customer_id, "checkout.session.completed: failed to update tenant");
        return;
    }

    state.evict_tenant(tenant_id);
    tracing::info!(%tenant_id, %customer_id, "tenant activated via checkout");
}

async fn handle_subscription_deleted(state: &AppState, sub: &stripe_shared::Subscription) {
    let customer_id = sub.customer.id().as_str().to_string();

    let cid = customer_id.clone();
    let tenant = match state
        .master_db
        .with_conn(move |conn| Tenant::find_by_stripe_customer_id(conn, &cid))
        .await
    {
        Ok(Some(t)) => t,
        Ok(None) => {
            tracing::warn!(%customer_id, "customer.subscription.deleted: no tenant");
            return;
        }
        Err(e) => {
            tracing::error!(?e, "customer.subscription.deleted: DB error");
            return;
        }
    };

    // Don't downgrade grandfathered tenants.
    if tenant.billing_status == BillingStatus::Grandfathered {
        tracing::info!(tenant_id = %tenant.id, "subscription deleted but tenant is grandfathered — skipping");
        return;
    }

    let tenant_id = tenant.id;
    if let Err(e) = state
        .master_db
        .with_conn(move |conn| {
            Tenant::set_billing_status(conn, tenant_id, BillingStatus::Free)?;
            Tenant::clear_stripe_subscription(conn, tenant_id)?;
            Ok::<_, diesel::result::Error>(())
        })
        .await
    {
        tracing::error!(?e, %customer_id, "customer.subscription.deleted: failed to update tenant");
        return;
    }

    state.evict_tenant(tenant_id);
    tracing::info!(%tenant_id, %customer_id, "tenant downgraded — subscription deleted");
}

async fn handle_payment_failed(state: &AppState, invoice: &stripe_shared::Invoice) {
    let customer_id = match &invoice.customer {
        Some(expandable) => expandable.id().as_str().to_string(),
        None => {
            tracing::warn!("invoice.payment_failed: no customer on invoice");
            return;
        }
    };

    let cid = customer_id.clone();
    let tenant = match state
        .master_db
        .with_conn(move |conn| Tenant::find_by_stripe_customer_id(conn, &cid))
        .await
    {
        Ok(Some(t)) => t,
        Ok(None) => {
            tracing::warn!(%customer_id, "invoice.payment_failed: no tenant");
            return;
        }
        Err(e) => {
            tracing::error!(?e, "invoice.payment_failed: DB error");
            return;
        }
    };

    if tenant.billing_status == BillingStatus::Grandfathered {
        return;
    }

    let tenant_id = tenant.id;
    if let Err(e) = state
        .master_db
        .with_conn(move |conn| Tenant::set_billing_status(conn, tenant_id, BillingStatus::Free))
        .await
    {
        tracing::error!(?e, %customer_id, "invoice.payment_failed: failed to update tenant");
        return;
    }

    state.evict_tenant(tenant_id);
    tracing::info!(%tenant_id, %customer_id, "tenant downgraded — payment failed");
}
