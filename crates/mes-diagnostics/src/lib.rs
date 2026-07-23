//! `mes-diagnostics` — mirrors `dnc-daemon`'s diagnostics module shape 1:1 (§8.5,
//! §12 M14): `heartbeat`, `manual`, `error_trigger`, `redact`, `buffer`, `crash`,
//! plus GitHub shipping.
//!
//! Redaction is **stricter** here than in DNC — MES diagnostics can carry
//! production counts, scrap reasons, part numbers, customer names, pricing, and
//! raw inspection values, so [`redact`] is an allowlist that keeps only
//! structural/error data (§8.5). Shipping is **opt-in per customer** (§17 Q4):
//! nothing leaves the box unless the customer turns it on, and everything that
//! does is redacted first — the single choke point is [`send_diagnostics`].

#![forbid(unsafe_code)]

pub mod buffer;
pub mod crash;
pub mod error_trigger;
pub mod github;
pub mod heartbeat;
pub mod manual;
pub mod redact;

use serde_json::Value;

pub use github::{GitHubShipper, ShipConfig, Shipper};

#[derive(Debug, thiserror::Error)]
pub enum DiagnosticsError {
    #[error("diagnostics error: {0}")]
    Failed(String),
    #[error("diagnostics shipping failed: {0}")]
    Ship(String),
}

/// What a send attempt did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    /// Shipping is disabled for this customer — nothing left the box (§8.5).
    Skipped,
    /// The redacted bundle was shipped.
    Shipped,
}

/// The single path a diagnostic bundle takes off the box. It **always redacts**
/// before shipping and honours the opt-in switch, so there is exactly one place
/// to audit for leaks (§8.5).
pub async fn send_diagnostics(
    shipper: &dyn Shipper,
    config: &ShipConfig,
    payload: &Value,
) -> Result<SendOutcome, DiagnosticsError> {
    if !config.enabled {
        return Ok(SendOutcome::Skipped);
    }
    let redacted = redact::redact(payload);
    let service = redacted
        .get("service")
        .and_then(Value::as_str)
        .unwrap_or("mes");
    let event = redacted
        .get("event")
        .and_then(Value::as_str)
        .unwrap_or("diagnostics");
    let title = format!("[diagnostics] {service}: {event}");
    let body = format!(
        "```json\n{}\n```",
        serde_json::to_string_pretty(&redacted).unwrap_or_default()
    );
    shipper.ship(&title, &body).await?;
    Ok(SendOutcome::Shipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::Mutex;

    /// Captures what would be shipped — no network.
    #[derive(Default)]
    struct MockShipper {
        sent: Mutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl Shipper for MockShipper {
        async fn ship(&self, title: &str, body: &str) -> Result<(), DiagnosticsError> {
            self.sent.lock().unwrap().push((title.into(), body.into()));
            Ok(())
        }
    }

    fn cfg(enabled: bool) -> ShipConfig {
        ShipConfig {
            enabled,
            repo: "acme/mes-diagnostics".into(),
            token: "t".into(),
        }
    }

    #[tokio::test]
    async fn disabled_ships_nothing() {
        let m = MockShipper::default();
        let out = send_diagnostics(&m, &cfg(false), &json!({"event": "manual"}))
            .await
            .unwrap();
        assert_eq!(out, SendOutcome::Skipped);
        assert!(m.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn manual_send_round_trips_redacted() {
        let payload = json!({
            "event": "manual",
            "service": "mes-edge",
            "version": "0.1.0",
            "message": "stall after op",
            "part_number": "PN-SECRET-9",
            "customer_name": "Acme Aerospace",
            "measured_value": 10.42
        });
        let m = MockShipper::default();
        let out = send_diagnostics(&m, &cfg(true), &payload).await.unwrap();
        assert_eq!(out, SendOutcome::Shipped);

        let sent = m.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let (title, body) = &sent[0];
        assert!(title.contains("mes-edge"));
        // The shipped body carries no business data.
        for leak in ["PN-SECRET-9", "Acme Aerospace", "10.42"] {
            assert!(!body.contains(leak), "leaked {leak} to the shipped bundle");
        }
        assert!(body.contains("stall after op"));
    }
}
