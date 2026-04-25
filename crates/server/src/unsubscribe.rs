//! Stateless HMAC-signed unsubscribe tokens for email opt-out.
//!
//! Tokens are computed from `(tenant_slug, user_id, email_type)` using
//! the JWT secret. They never expire and are idempotent — clicking the
//! same link twice is harmless.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::jwt::JwtKeys;
use crate::state::MailerCtx;
use lineup_db::app_user::UserId;

type HmacSha256 = Hmac<Sha256>;

/// The kinds of email a user can unsubscribe from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmailType {
    Reminders,
    Lineups,
    StaleAlerts,
    All,
}

impl EmailType {
    pub(crate) fn from_str(s: &str) -> Option<Self> {
        match s {
            "reminders" => Some(Self::Reminders),
            "lineups" => Some(Self::Lineups),
            "stale_alerts" => Some(Self::StaleAlerts),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Reminders => "reminders",
            Self::Lineups => "lineups",
            Self::StaleAlerts => "stale_alerts",
            Self::All => "all",
        }
    }
}

/// Compute an HMAC-SHA256 signature over `"{slug}:{user_id}:{email_type}"`.
pub(crate) fn sign(jwt_keys: &JwtKeys, slug: &str, user_id: UserId, email_type: &str) -> String {
    let msg = format!("{slug}:{user_id}:{email_type}");
    let mut mac =
        HmacSha256::new_from_slice(jwt_keys.secret_bytes()).expect("HMAC accepts any key size");
    mac.update(msg.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Verify a signature. Constant-time comparison via the `hmac` crate.
pub(crate) fn verify(
    jwt_keys: &JwtKeys,
    slug: &str,
    user_id: UserId,
    email_type: &str,
    signature: &str,
) -> bool {
    let expected = sign(jwt_keys, slug, user_id, email_type);
    // Both are hex strings of equal length; use constant-time compare.
    constant_time_eq(expected.as_bytes(), signature.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Build the full unsubscribe URL for a given email type.
pub(crate) fn url(
    mailer_ctx: &MailerCtx,
    slug: &str,
    user_id: UserId,
    email_type: EmailType,
    jwt_keys: &JwtKeys,
) -> String {
    let sig = sign(jwt_keys, slug, user_id, email_type.as_str());
    mailer_ctx.full_url(&format!(
        "/unsubscribe/{slug}/{user_id}/{}/{sig}",
        email_type.as_str()
    ))
}
