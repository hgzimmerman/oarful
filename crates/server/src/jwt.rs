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

/// Sentinel value for the superuser identity in the `sub` claim.
const SUPERUSER_SUB: i32 = -1;

/// Who the token belongs to: a real tenant user or the global superuser.
/// Serializes as a plain integer in the JWT `sub` claim: positive values
/// are user IDs, `-1` is the superuser sentinel. This keeps `sub` as a
/// simple numeric type that `jsonwebtoken` can validate without choking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Identity {
    User(UserId),
    Superuser,
}

impl Serialize for Identity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Identity::User(id) => serializer.serialize_i32(id.as_int()),
            Identity::Superuser => serializer.serialize_i32(SUPERUSER_SUB),
        }
    }
}

impl<'de> Deserialize<'de> for Identity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let n = i32::deserialize(deserializer)?;
        if n == SUPERUSER_SUB {
            Ok(Identity::Superuser)
        } else {
            Ok(Identity::User(UserId::new(n)))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Claims {
    /// Who this token belongs to. Positive = user ID, -1 = superuser.
    sub: Identity,
    /// Which tenant (club) this token is for.
    tenant_id: TenantId,
    /// Tenant-wide role.
    role: Role,
    /// Which team the user is currently viewing.
    active_team_id: TeamId,
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
        let now = now_secs();
        Self {
            sub: Identity::User(user_id),
            tenant_id,
            role,
            active_team_id,
            exp: now + TOKEN_LIFETIME_SECS,
            iat: now,
        }
    }

    /// The real user ID within the tenant DB, or `None` for superuser.
    pub(crate) fn user_id(&self) -> Option<UserId> {
        match self.sub {
            Identity::User(id) => Some(id),
            Identity::Superuser => None,
        }
    }

    /// Shorthand for audit logging — alias for `user_id()`.
    pub(crate) fn audit_user_id(&self) -> Option<UserId> {
        self.user_id()
    }

    pub(crate) fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }

    pub(crate) fn team_id(&self) -> TeamId {
        self.active_team_id
    }

    pub(crate) fn role(&self) -> Role {
        self.role
    }

    pub(crate) fn is_superuser(&self) -> bool {
        matches!(self.sub, Identity::Superuser)
    }

    fn new_superuser(lifetime_secs: u64) -> Self {
        let now = now_secs();
        Self {
            sub: Identity::Superuser,
            tenant_id: TenantId::new(0),
            role: Role::ProgramDirector,
            active_team_id: TeamId::new(0),
            exp: now + lifetime_secs,
            iat: now,
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
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
        let now = now_secs();
        let claims = Claims {
            sub: Identity::Superuser,
            tenant_id,
            role: Role::ProgramDirector,
            active_team_id,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keys() -> JwtKeys {
        JwtKeys::from_secret(b"test-secret-key")
    }

    #[test]
    fn issue_and_verify_round_trip() {
        let keys = test_keys();
        let token = keys
            .issue(
                UserId::new(1),
                TenantId::new(2),
                Role::Coach,
                TeamId::new(3),
            )
            .unwrap();
        let claims = keys.verify(&token).unwrap();
        assert_eq!(claims.user_id(), Some(UserId::new(1)));
        assert_eq!(claims.tenant_id(), TenantId::new(2));
        assert_eq!(claims.role(), Role::Coach);
        assert_eq!(claims.team_id(), TeamId::new(3));
        assert!(!claims.is_superuser());
    }

    #[test]
    fn issue_with_expiry_round_trip() {
        let keys = test_keys();
        let future_exp = now_secs() + 3600;
        let token = keys
            .issue_with_expiry(
                UserId::new(5),
                TenantId::new(1),
                Role::ProgramDirector,
                TeamId::new(1),
                future_exp,
            )
            .unwrap();
        let claims = keys.verify(&token).unwrap();
        assert_eq!(claims.user_id(), Some(UserId::new(5)));
        assert_eq!(claims.role(), Role::ProgramDirector);
    }

    #[test]
    fn superuser_round_trip() {
        let keys = test_keys();
        let token = keys.issue_superuser().unwrap();
        let claims = keys.verify(&token).unwrap();
        assert!(claims.is_superuser());
        assert!(claims.user_id().is_none());
    }

    #[test]
    fn wrong_secret_rejects() {
        let keys = test_keys();
        let token = keys
            .issue(
                UserId::new(1),
                TenantId::new(1),
                Role::Member,
                TeamId::new(1),
            )
            .unwrap();
        let other_keys = JwtKeys::from_secret(b"different-secret");
        assert!(other_keys.verify(&token).is_err());
    }

    #[test]
    fn expired_token_rejects() {
        let keys = test_keys();
        let past_exp = now_secs() - 120; // well past the 60s leeway
        let token = keys
            .issue_with_expiry(
                UserId::new(1),
                TenantId::new(1),
                Role::Member,
                TeamId::new(1),
                past_exp,
            )
            .unwrap();
        assert!(keys.verify(&token).is_err());
    }

    #[test]
    fn garbage_token_rejects() {
        let keys = test_keys();
        assert!(keys.verify("not.a.jwt").is_err());
        assert!(keys.verify("").is_err());
    }
}
