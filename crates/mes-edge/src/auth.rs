//! Authentication primitives: password/PIN hashing (argon2) and JWTs.
//!
//! Secrets are argon2id PHC strings hashed server-side and stored at rest
//! (§14). Bearer tokens are HS256 JWTs signed with a server secret from the
//! environment. This module is pure crypto/token logic — no DB access — so the
//! DB layer and the HTTP layer both depend on it without cycles.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Auth configuration resolved from the environment.
#[derive(Clone)]
pub struct AuthConfig {
    secret: String,
    ttl_secs: i64,
}

impl AuthConfig {
    pub fn new(secret: String, ttl_secs: i64) -> Self {
        Self { secret, ttl_secs }
    }

    /// The signing secret's bytes. Used to derive the ERP token-encryption key
    /// (domain-separated, §14) so there is a single configured secret.
    pub fn secret_bytes(&self) -> &[u8] {
        self.secret.as_bytes()
    }
}

/// JWT claims. `sub` is the user id, `role` the role code, `exp`/`iat` epoch
/// seconds. `role` is embedded so authorization checks need no DB round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub role: String,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("hashing failed")]
    Hash,
    #[error("token encoding failed")]
    Encode,
    #[error("invalid or expired token")]
    InvalidToken,
}

/// Hash a password or PIN into an argon2id PHC string.
pub fn hash_secret(plain: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|_| AuthError::Hash)?
        .to_string();
    Ok(hash)
}

/// Verify a plaintext secret against a stored argon2 PHC string. Any parse or
/// mismatch yields `false` — never an error, so callers can't accidentally leak
/// which half failed.
pub fn verify_secret(plain: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// Issue a signed token for a user. Returns the token and its expiry epoch.
pub fn issue_token(
    cfg: &AuthConfig,
    user_id: &str,
    role: &str,
) -> Result<(String, i64), AuthError> {
    let now = Utc::now().timestamp();
    let exp = now + cfg.ttl_secs;
    let claims = Claims {
        sub: user_id.to_string(),
        role: role.to_string(),
        iat: now,
        exp,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(cfg.secret.as_bytes()),
    )
    .map_err(|_| AuthError::Encode)?;
    Ok((token, exp))
}

/// Decode and validate a bearer token, returning its claims.
pub fn decode_token(cfg: &AuthConfig, token: &str) -> Result<Claims, AuthError> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(cfg.secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| AuthError::InvalidToken)?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_roundtrip_verifies() {
        let phc = hash_secret("s3cret").expect("hash");
        assert!(verify_secret("s3cret", &phc));
        assert!(!verify_secret("wrong", &phc));
    }

    #[test]
    fn garbage_hash_never_verifies() {
        assert!(!verify_secret("anything", "not-a-phc-string"));
    }

    #[test]
    fn token_roundtrips_and_carries_role() {
        let cfg = AuthConfig::new("test-secret".to_string(), 3600);
        let (token, exp) = issue_token(&cfg, "user-1", "Admin").expect("issue");
        assert!(exp > Utc::now().timestamp());
        let claims = decode_token(&cfg, &token).expect("decode");
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.role, "Admin");
    }

    #[test]
    fn token_from_other_secret_is_rejected() {
        let a = AuthConfig::new("secret-a".to_string(), 3600);
        let b = AuthConfig::new("secret-b".to_string(), 3600);
        let (token, _) = issue_token(&a, "user-1", "Admin").expect("issue");
        assert!(matches!(
            decode_token(&b, &token),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn expired_token_is_rejected() {
        // Beyond jsonwebtoken's default 60s validation leeway.
        let cfg = AuthConfig::new("test-secret".to_string(), -120);
        let (token, _) = issue_token(&cfg, "user-1", "Operator").expect("issue");
        assert!(matches!(
            decode_token(&cfg, &token),
            Err(AuthError::InvalidToken)
        ));
    }
}
