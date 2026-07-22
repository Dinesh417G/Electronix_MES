//! `mes-client` — shared API types and (later) a typed HTTP client.
//!
//! Both `mes-edge` and `mes-cloud` serialise these types, and the Tauri apps
//! deserialise them, so the wire contract lives in exactly one place. Kept
//! dependency-light and mobile-ready (§1): no async runtime, no I/O here.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Liveness/version payload returned by `/healthz` on every service.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    /// Which service answered — `"mes-edge"` or `"mes-cloud"`.
    pub service: String,
    /// Always `"ok"` when the process can serve requests.
    pub status: String,
    /// Crate version of the answering binary.
    pub version: String,
}

impl HealthResponse {
    /// Build an `ok` health response for the named service.
    pub fn ok(service: &str, version: &str) -> Self {
        Self {
            service: service.to_string(),
            status: "ok".to_string(),
            version: version.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_helper_sets_status() {
        let h = HealthResponse::ok("mes-edge", "0.1.0");
        assert_eq!(h.status, "ok");
        assert_eq!(h.service, "mes-edge");
    }
}
