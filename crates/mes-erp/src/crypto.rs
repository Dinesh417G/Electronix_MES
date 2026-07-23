//! Symmetric encryption for ERP auth tokens at rest (§14).
//!
//! Tokens must be recoverable (we send them to the ERP), so this is reversible
//! AEAD encryption, not a one-way hash. The key is *derived* from the server's
//! signing secret with domain separation, so there is exactly one secret to
//! configure (via env, §14) and the derived key is stable across restarts.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use sha2::{Digest, Sha256};

use crate::ErpError;

const NONCE_LEN: usize = 24;

/// Derive a 32-byte encryption key from the server secret. Domain-separated so
/// it never collides with the same secret's other uses (e.g. JWT signing).
pub fn derive_key(secret: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"mes-erp-token-encryption-v1");
    h.update(secret);
    h.finalize().into()
}

/// Encrypt a token, returning base64(`nonce || ciphertext`).
pub fn encrypt(key: &[u8; 32], plaintext: &str) -> Result<String, ErpError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce).map_err(|_| ErpError::Crypto)?;
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|_| ErpError::Crypto)?;
    let mut buf = Vec::with_capacity(NONCE_LEN + ct.len());
    buf.extend_from_slice(&nonce);
    buf.extend_from_slice(&ct);
    Ok(STANDARD.encode(buf))
}

/// Decrypt a base64(`nonce || ciphertext`) token produced by [`encrypt`].
pub fn decrypt(key: &[u8; 32], token: &str) -> Result<String, ErpError> {
    let buf = STANDARD.decode(token).map_err(|_| ErpError::Crypto)?;
    if buf.len() <= NONCE_LEN {
        return Err(ErpError::Crypto);
    }
    let (nonce, ct) = buf.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(key.into());
    let pt = cipher
        .decrypt(XNonce::from_slice(nonce), ct)
        .map_err(|_| ErpError::Crypto)?;
    String::from_utf8(pt).map_err(|_| ErpError::Crypto)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_token() {
        let key = derive_key(b"server-secret");
        let ct = encrypt(&key, "erp-api-token-123").unwrap();
        // Ciphertext is not the plaintext and is base64.
        assert_ne!(ct, "erp-api-token-123");
        assert_eq!(decrypt(&key, &ct).unwrap(), "erp-api-token-123");
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let ct = encrypt(&derive_key(b"secret-a"), "tok").unwrap();
        assert!(decrypt(&derive_key(b"secret-b"), &ct).is_err());
    }

    #[test]
    fn nonce_randomises_ciphertext() {
        let key = derive_key(b"s");
        assert_ne!(
            encrypt(&key, "same").unwrap(),
            encrypt(&key, "same").unwrap()
        );
    }

    #[test]
    fn derive_key_is_deterministic_and_domain_separated() {
        assert_eq!(derive_key(b"x"), derive_key(b"x"));
        assert_ne!(derive_key(b"x"), derive_key(b"y"));
    }
}
