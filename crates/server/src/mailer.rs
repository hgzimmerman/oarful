//! Abstraction over invite delivery so the server doesn't hard-code a
//! specific email provider. Production deploys supply a real
//! implementation (SendGrid, SES, SMTP, …); dev and self-hosted
//! setups use [`LogMailer`] which just prints the invite URL.

use anyhow::Result;

/// Async-capable interface for delivering invite links to users.
///
/// Implementations must be `Send + Sync` so they can live on
/// [`crate::state::AppState`] behind an `Arc`.
#[async_trait::async_trait]
pub trait Mailer: Send + Sync {
    /// Deliver an invite link to a single recipient.
    ///
    /// Implementations should be best-effort: a delivery failure is
    /// logged but does not prevent the invite from being created (the
    /// UI still shows the link as a fallback).
    async fn send_invite(&self, to_email: &str, to_name: &str, invite_url: &str) -> Result<()>;
}

/// Development mailer that logs the invite URL via `tracing::info!`
/// instead of actually sending anything. Good for local dev and
/// self-hosted deployments where the PD can copy the link from the
/// server logs or the UI.
pub struct LogMailer;

#[async_trait::async_trait]
impl Mailer for LogMailer {
    async fn send_invite(&self, to_email: &str, to_name: &str, invite_url: &str) -> Result<()> {
        tracing::info!(
            to_email,
            to_name,
            invite_url,
            "invite ready (LogMailer — no email sent)"
        );
        Ok(())
    }
}
