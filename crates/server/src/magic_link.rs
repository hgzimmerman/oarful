//! Magic link creation helper. Generates a random token, SHA-256
//! hashes it for DB storage, and returns the raw token for use in
//! URLs.

use chrono::NaiveDateTime;
use lineup_db::app_user::UserId;
use lineup_db::magic_link::NewMagicLink;
use lineup_db::team::TeamId;
use sha2::{Digest, Sha256};

/// Generate a random 32-hex-char token (same entropy as invite tokens).
fn generate_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let a = RandomState::new().build_hasher().finish();
    let b = RandomState::new().build_hasher().finish();
    format!("{a:016x}{b:016x}")
}

/// SHA-256 hash a raw token, returning the hex digest.
pub(crate) fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Result of creating a magic link: the raw token (for URLs) and the
/// DB row ready for insertion.
pub(crate) struct CreatedMagicLink {
    /// The raw token to embed in the URL. NOT stored in the DB.
    pub(crate) raw_token: String,
    /// The insertable row (with hashed token).
    pub(crate) row: NewMagicLink,
}

/// Create a magic link for a user. Returns the raw token and the
/// insertable DB row. The caller is responsible for actually
/// inserting the row.
pub(crate) fn create_magic_link(
    user_id: UserId,
    redirect_path: &str,
    expires_at: NaiveDateTime,
    team_id: Option<TeamId>,
) -> CreatedMagicLink {
    let raw_token = generate_token();
    let token_hash = hash_token(&raw_token);
    let now = chrono::Utc::now().naive_utc();

    CreatedMagicLink {
        raw_token,
        row: NewMagicLink {
            token_hash,
            user_id,
            redirect_path: redirect_path.to_string(),
            expires_at,
            created_at: now,
            team_id,
        },
    }
}
