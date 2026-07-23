//! `mes-client` — shared API types and (later) a typed HTTP client.
//!
//! Both `mes-edge` and `mes-cloud` serialise these types, and the Tauri apps
//! deserialise them, so the wire contract lives in exactly one place. Kept
//! dependency-light and mobile-ready (§1): no async runtime, no I/O here.

#![forbid(unsafe_code)]

pub mod analytics;
pub mod auth;
pub mod cmms;
pub mod copilot;
pub mod dnc;
pub mod erp;
pub mod exec;
pub mod ingest;
pub mod master;
pub mod orders;
pub mod qms;
pub mod sync;
pub mod trace;
pub mod ws;

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

/// Standard error envelope returned by the API on non-2xx responses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiError {
    /// Machine-readable error code, e.g. `"unauthorized"`, `"forbidden"`.
    pub error: String,
    /// Human-readable detail.
    pub message: String,
}

impl ApiError {
    pub fn new(error: &str, message: impl Into<String>) -> Self {
        Self {
            error: error.to_string(),
            message: message.into(),
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
