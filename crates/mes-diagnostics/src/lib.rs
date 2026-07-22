//! `mes-diagnostics` — mirrors `dnc-daemon`'s diagnostics module shape 1:1 (§8.5, M14).
//!
//! Planned modules (built in M14): `heartbeat`, `manual`, `error_trigger`,
//! `redact`, `buffer`, `crash`. Redaction is **stricter** here than in DNC:
//! MES diagnostics can carry production counts, scrap reasons, and business
//! data, so `redact` uses a structural/error-only allowlist and never emits
//! customer part numbers, names, pricing, or raw inspection values (§8.5).
//! Shipping is **opt-in per customer** (§17 Q4). M0 lands only the crate shell.

#![forbid(unsafe_code)]

#[derive(Debug, thiserror::Error)]
pub enum DiagnosticsError {
    #[error("diagnostics error: {0}")]
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_renders() {
        let e = DiagnosticsError::Failed("boom".into());
        assert_eq!(e.to_string(), "diagnostics error: boom");
    }
}
