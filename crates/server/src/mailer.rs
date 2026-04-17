//! Abstraction over email delivery so the server doesn't hard-code a
//! specific email provider. Production deploys supply a real
//! implementation (SendGrid, SES, SMTP, …); dev and self-hosted
//! setups use [`LogMailer`] which just prints the content.

use anyhow::Result;
use chrono::NaiveDate;

/// A boat's lineup for email rendering — boat name + ordered seat list.
#[derive(Debug, Clone)]
pub struct EmailBoatLineup {
    pub boat_name: String,
    pub seats: Vec<EmailSeat>,
}

/// A single seat in an email lineup.
#[derive(Debug, Clone)]
pub struct EmailSeat {
    pub label: String,
    pub rower_name: String,
}

/// Summary of lineups for one practice date.
#[derive(Debug, Clone)]
pub struct EmailLineupSummary {
    pub date: NaiveDate,
    pub boats: Vec<EmailBoatLineup>,
    /// Rowers who were benched (not placed in any boat).
    pub benched: Vec<String>,
}

/// Async-capable interface for delivering emails to users.
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

    /// Send an availability reminder to a single recipient.
    ///
    /// `dates` lists practice dates the rower hasn't responded to.
    /// `magic_url` is a magic-link URL to `/my/availability`.
    async fn send_reminder(
        &self,
        to_email: &str,
        to_name: &str,
        team_name: &str,
        dates: &[NaiveDate],
        magic_url: &str,
        unsubscribe_url: &str,
        unsubscribe_all_url: &str,
    ) -> Result<()>;

    /// Send a magic-link login email. Each entry in `clubs` is a
    /// (club_name, magic_url) pair. When there's only one club the
    /// email has a single "Sign in" button; when there are multiple
    /// the user picks which club to sign into.
    async fn send_magic_login(
        &self,
        to_email: &str,
        to_name: &str,
        clubs: &[(String, String)],
    ) -> Result<()>;

    /// Send a lineup notification to a single recipient.
    ///
    /// `lineups` contains the full seat assignments per boat per date.
    /// `magic_url` is a magic-link URL to `/history/{date}`.
    async fn send_lineup(
        &self,
        to_email: &str,
        to_name: &str,
        team_name: &str,
        lineups: &[EmailLineupSummary],
        magic_url: &str,
        unsubscribe_url: &str,
        unsubscribe_all_url: &str,
    ) -> Result<()>;
}

/// A captured email message for test assertions.
#[derive(Debug, Clone)]
pub enum MailMessage {
    Invite {
        to_email: String,
        to_name: String,
        invite_url: String,
    },
    Reminder {
        to_email: String,
        to_name: String,
        team_name: String,
        dates: Vec<NaiveDate>,
        magic_url: String,
        unsubscribe_url: String,
        unsubscribe_all_url: String,
    },
    MagicLogin {
        to_email: String,
        to_name: String,
        clubs: Vec<(String, String)>,
    },
    Lineup {
        to_email: String,
        to_name: String,
        team_name: String,
        lineups: Vec<EmailLineupSummary>,
        magic_url: String,
        unsubscribe_url: String,
        unsubscribe_all_url: String,
    },
}

/// Test mailer that sends structured [`MailMessage`]s through a channel
/// so tests can receive and assert on them. Also useful for extracting
/// magic-link tokens to log in as different users.
pub struct ChannelMailer {
    tx: tokio::sync::mpsc::UnboundedSender<MailMessage>,
}

impl ChannelMailer {
    /// Create a new channel mailer and its receiving half.
    pub fn new() -> (Self, tokio::sync::mpsc::UnboundedReceiver<MailMessage>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { tx }, rx)
    }
}

#[async_trait::async_trait]
impl Mailer for ChannelMailer {
    async fn send_invite(&self, to_email: &str, to_name: &str, invite_url: &str) -> Result<()> {
        let _ = self.tx.send(MailMessage::Invite {
            to_email: to_email.to_string(),
            to_name: to_name.to_string(),
            invite_url: invite_url.to_string(),
        });
        Ok(())
    }

    async fn send_magic_login(
        &self,
        to_email: &str,
        to_name: &str,
        clubs: &[(String, String)],
    ) -> Result<()> {
        let _ = self.tx.send(MailMessage::MagicLogin {
            to_email: to_email.to_string(),
            to_name: to_name.to_string(),
            clubs: clubs.to_vec(),
        });
        Ok(())
    }

    async fn send_reminder(
        &self,
        to_email: &str,
        to_name: &str,
        team_name: &str,
        dates: &[NaiveDate],
        magic_url: &str,
        unsubscribe_url: &str,
        unsubscribe_all_url: &str,
    ) -> Result<()> {
        let _ = self.tx.send(MailMessage::Reminder {
            to_email: to_email.to_string(),
            to_name: to_name.to_string(),
            team_name: team_name.to_string(),
            dates: dates.to_vec(),
            magic_url: magic_url.to_string(),
            unsubscribe_url: unsubscribe_url.to_string(),
            unsubscribe_all_url: unsubscribe_all_url.to_string(),
        });
        Ok(())
    }

    async fn send_lineup(
        &self,
        to_email: &str,
        to_name: &str,
        team_name: &str,
        lineups: &[EmailLineupSummary],
        magic_url: &str,
        unsubscribe_url: &str,
        unsubscribe_all_url: &str,
    ) -> Result<()> {
        let _ = self.tx.send(MailMessage::Lineup {
            to_email: to_email.to_string(),
            to_name: to_name.to_string(),
            team_name: team_name.to_string(),
            lineups: lineups.to_vec(),
            magic_url: magic_url.to_string(),
            unsubscribe_url: unsubscribe_url.to_string(),
            unsubscribe_all_url: unsubscribe_all_url.to_string(),
        });
        Ok(())
    }
}

/// Development mailer that logs email content via `tracing` instead
/// of actually sending anything. Good for local dev and self-hosted
/// deployments.
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

    async fn send_magic_login(
        &self,
        to_email: &str,
        to_name: &str,
        clubs: &[(String, String)],
    ) -> Result<()> {
        let club_names: Vec<&str> = clubs.iter().map(|(name, _)| name.as_str()).collect();
        let html = crate::templates::email::magic_login_email(to_name, clubs).into_string();
        tracing::info!(
            to_email,
            to_name,
            ?club_names,
            "magic login link (LogMailer -- no email sent)"
        );
        tracing::trace!(html, "magic login email HTML");
        Ok(())
    }

    async fn send_reminder(
        &self,
        to_email: &str,
        to_name: &str,
        team_name: &str,
        dates: &[NaiveDate],
        magic_url: &str,
        unsubscribe_url: &str,
        unsubscribe_all_url: &str,
    ) -> Result<()> {
        let date_list: Vec<String> = dates.iter().map(|d| d.to_string()).collect();
        let html = crate::templates::email::reminder_email(
            to_name,
            team_name,
            dates,
            magic_url,
            unsubscribe_url,
            unsubscribe_all_url,
        )
        .into_string();
        tracing::info!(
            to_email,
            to_name,
            team_name,
            ?date_list,
            unsubscribe_url,
            "availability reminder (LogMailer — no email sent)"
        );
        tracing::trace!(html, "reminder email HTML");
        Ok(())
    }

    async fn send_lineup(
        &self,
        to_email: &str,
        to_name: &str,
        team_name: &str,
        lineups: &[EmailLineupSummary],
        magic_url: &str,
        unsubscribe_url: &str,
        unsubscribe_all_url: &str,
    ) -> Result<()> {
        let dates: Vec<String> = lineups.iter().map(|l| l.date.to_string()).collect();
        let html = crate::templates::email::lineup_email(
            to_name,
            team_name,
            lineups,
            magic_url,
            unsubscribe_url,
            unsubscribe_all_url,
        )
        .into_string();
        tracing::info!(
            to_email,
            to_name,
            team_name,
            ?dates,
            unsubscribe_url,
            "lineup notification (LogMailer — no email sent)"
        );
        tracing::trace!(html, "lineup email HTML");
        Ok(())
    }
}
