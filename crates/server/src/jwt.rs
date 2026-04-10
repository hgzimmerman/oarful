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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Claims {
    /// User ID within the tenant DB.
    pub(crate) sub: i32,
    /// Which tenant (club) this token is for.
    pub(crate) tenant_id: i32,
    /// Tenant-wide role: "Member", "Coach", "ProgramDirector".
    pub(crate) role: String,
    /// Which team the user is currently viewing.
    pub(crate) active_team_id: i32,
    /// Expiry (Unix timestamp).
    pub(crate) exp: u64,
    /// Issued at (Unix timestamp).
    pub(crate) iat: u64,
}

impl Claims {
    pub(crate) fn user_id(&self) -> UserId {
        UserId::new(self.sub)
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
}

/// Shared JWT signing/verification keys, built once at startup.
#[derive(Clone)]
pub(crate) struct JwtKeys {
    encoding: Arc<EncodingKey>,
    decoding: Arc<DecodingKey>,
}

impl JwtKeys {
    pub(crate) fn from_secret(secret: &[u8]) -> Self {
        Self {
            encoding: Arc::new(EncodingKey::from_secret(secret)),
            decoding: Arc::new(DecodingKey::from_secret(secret)),
        }
    }

    /// Issue a new token for the given user.
    pub(crate) fn issue(
        &self,
        user_id: UserId,
        tenant_id: TenantId,
        role: Role,
        active_team_id: TeamId,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let claims = Claims {
            sub: user_id.as_int(),
            tenant_id: tenant_id.as_int(),
            role: role.as_str().to_string(),
            active_team_id: active_team_id.as_int(),
            exp: now + TOKEN_LIFETIME_SECS,
            iat: now,
        };
        jsonwebtoken::encode(&Header::default(), &claims, &self.encoding)
    }

    /// Decode and validate a token. Returns the claims on success.
    pub(crate) fn verify(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let data = jsonwebtoken::decode::<Claims>(
            token,
            &self.decoding,
            &Validation::default(),
        )?;
        Ok(data.claims)
    }
}
