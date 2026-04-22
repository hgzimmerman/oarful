//! JWT token issuance and validation.
//!
//! Tokens are HS256-signed with `JWT_SECRET`. The server reads this
//! from the environment at startup; if unset, a random secret is
//! generated (tokens won't survive a restart, which is acceptable
//! for dev).

use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use lineup_db::app_user::{Role, UserId};
use lineup_db::team::TeamId;
use lineup_master_db::tenant::TenantId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// JWT lifetime in seconds (24 hours).
const TOKEN_LIFETIME_SECS: u64 = 86400;

/// Who the token belongs to: a real tenant user or the global superuser.
/// Serializes as `{"identity": "user", "id": 42}` or `{"identity": "superuser"}`,
/// flattened into the JWT claims object.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "identity", content = "id")]
pub(crate) enum Identity {
    #[serde(rename = "user")]
    User(i32),
    #[serde(rename = "superuser")]
    Superuser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Claims {
    /// Who this token belongs to.
    sub: Identity,
    /// Which tenant (club) this token is for (0 for pure superuser).
    tenant_id: i32,
    /// Tenant-wide role: "Member", "Coach", "ProgramDirector", "Superuser".
    role: String,
    /// Which team the user is currently viewing (0 for pure superuser).
    active_team_id: i32,
    /// Expiry (Unix timestamp).
    exp: u64,
    /// Issued at (Unix timestamp).
    iat: u64,
}

impl Claims {
    pub(crate) fn new(
        user_id: UserId,
        tenant_id: TenantId,
        role: Role,
        active_team_id: TeamId,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            sub: Identity::User(user_id.as_int()),
            tenant_id: tenant_id.as_int(),
            role: role.as_str().to_string(),
            active_team_id: active_team_id.as_int(),
            exp: now + TOKEN_LIFETIME_SECS,
            iat: now,
        }
    }

    /// The real user ID within the tenant DB, or `None` for superuser.
    pub(crate) fn user_id(&self) -> Option<UserId> {
        match self.sub {
            Identity::User(id) => Some(UserId::new(id)),
            Identity::Superuser => None,
        }
    }

    /// Shorthand for audit logging: returns `Some(user_id)` as `i32`
    /// for real users, `None` for superuser sessions.
    pub(crate) fn audit_user_id(&self) -> Option<i32> {
        match self.sub {
            Identity::User(id) => Some(id),
            Identity::Superuser => None,
        }
    }

    pub(crate) fn tenant_id(&self) -> TenantId {
        TenantId::new(self.tenant_id)
    }

    pub(crate) fn team_id(&self) -> TeamId {
        TeamId::new(self.active_team_id)
    }

    pub(crate) fn role(&self) -> Option<Role> {
        Role::from_str(&self.role)
    }

    pub(crate) fn is_superuser(&self) -> bool {
        matches!(self.sub, Identity::Superuser)
    }

    fn new_superuser(lifetime_secs: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            sub: Identity::Superuser,
            tenant_id: 0,
            role: "Superuser".to_string(),
            active_team_id: 0,
            exp: now + lifetime_secs,
            iat: now,
        }
    }
}

/// Shared JWT signing/verification keys, built once at startup.
#[derive(Clone)]
pub(crate) struct JwtKeys {
    encoding: Arc<EncodingKey>,
    decoding: Arc<DecodingKey>,
    /// Raw secret bytes, used for HMAC signing of unsubscribe tokens.
    secret: Arc<Vec<u8>>,
}

impl JwtKeys {
    pub(crate) fn from_secret(secret: &[u8]) -> Self {
        Self {
            encoding: Arc::new(EncodingKey::from_secret(secret)),
            decoding: Arc::new(DecodingKey::from_secret(secret)),
            secret: Arc::new(secret.to_vec()),
        }
    }

    /// Raw secret bytes for HMAC signing (e.g. unsubscribe tokens).
    pub(crate) fn secret_bytes(&self) -> &[u8] {
        &self.secret
    }

    /// Issue a new token for the given user.
    pub(crate) fn issue(
        &self,
        user_id: UserId,
        tenant_id: TenantId,
        role: Role,
        active_team_id: TeamId,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let claims = Claims::new(user_id, tenant_id, role, active_team_id);
        jsonwebtoken::encode(&Header::default(), &claims, &self.encoding)
    }

    /// Issue a token with a custom expiry timestamp (Unix seconds).
    /// Used for magic-link sessions where the JWT should expire at
    /// end-of-day of the last relevant practice.
    pub(crate) fn issue_with_expiry(
        &self,
        user_id: UserId,
        tenant_id: TenantId,
        role: Role,
        active_team_id: TeamId,
        exp: u64,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let mut claims = Claims::new(user_id, tenant_id, role, active_team_id);
        claims.exp = exp;
        jsonwebtoken::encode(&Header::default(), &claims, &self.encoding)
    }

    /// Issue a 24h superuser session token.
    pub(crate) fn issue_superuser(&self) -> Result<String, jsonwebtoken::errors::Error> {
        let claims = Claims::new_superuser(TOKEN_LIFETIME_SECS);
        jsonwebtoken::encode(&Header::default(), &claims, &self.encoding)
    }

    /// Issue a short-lived (10 min) superuser token for magic link URLs.
    pub(crate) fn issue_superuser_magic_token(
        &self,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let claims = Claims::new_superuser(600);
        jsonwebtoken::encode(&Header::default(), &claims, &self.encoding)
    }

    /// Issue a superuser tenant-view token. The superuser gets PD-level
    /// access without taking over a real user's identity.
    pub(crate) fn issue_superuser_tenant_view(
        &self,
        tenant_id: TenantId,
        active_team_id: TeamId,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let claims = Claims {
            sub: Identity::Superuser,
            tenant_id: tenant_id.as_int(),
            role: Role::ProgramDirector.as_str().to_string(),
            active_team_id: active_team_id.as_int(),
            exp: now + TOKEN_LIFETIME_SECS,
            iat: now,
        };
        jsonwebtoken::encode(&Header::default(), &claims, &self.encoding)
    }

    /// Decode and validate a token. Returns the claims on success.
    pub(crate) fn verify(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let data = jsonwebtoken::decode::<Claims>(token, &self.decoding, &Validation::default())?;
        Ok(data.claims)
    }
}
