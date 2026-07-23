//! `mes-erp` — generic ERP import/export adapter + field-mapping engine (§8, M10).
//!
//! No per-customer code (§3): the admin integration page stores an endpoint, a
//! token (encrypted at rest, §14), and a JSONB field-mapping; this crate applies
//! that mapping to round-trip records against whatever REST shape the customer's
//! ERP exposes, and pushes/pulls over a generic REST client. Re-pointing at a
//! differently-shaped ERP is a mapping change only — never a code change.

#![forbid(unsafe_code)]

pub mod crypto;
pub mod mapping;
pub mod push;

pub use mapping::FieldMapping;
pub use push::ErpClient;

#[derive(Debug, thiserror::Error)]
pub enum ErpError {
    #[error("field mapping error: {0}")]
    Mapping(String),
    #[error("token encryption error")]
    Crypto,
    #[error("erp request failed: {0}")]
    Http(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_renders() {
        let e = ErpError::Mapping("bad field".into());
        assert_eq!(e.to_string(), "field mapping error: bad field");
    }
}
