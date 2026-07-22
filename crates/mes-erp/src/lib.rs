//! `mes-erp` — generic ERP import/export adapter + field-mapping engine (§8, M10).
//!
//! No per-customer code (§3): the admin integration page stores an endpoint,
//! token, and a JSONB field-mapping; this crate applies that mapping to
//! round-trip records against whatever REST shape the customer's ERP exposes.
//! M0 lands only the crate shell; the mapping engine arrives in M10.

#![forbid(unsafe_code)]

#[derive(Debug, thiserror::Error)]
pub enum ErpError {
    #[error("field mapping error: {0}")]
    Mapping(String),
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
