//! `mes-ingest` — machine signal ingestion (§9).
//!
//! Defines the `SignalSource` trait that MQTT/HTTP/TCP-line/sim adapters
//! implement (M2). DNC transfer events do **not** flow through this path — they
//! are owned by `mes-dnc-bridge` (§8.4, §9). M0 lands only the trait shape;
//! adapters are built in M2.

#![forbid(unsafe_code)]

use async_trait::async_trait;

/// Errors an ingest adapter may raise while pulling signals.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("signal source unavailable: {0}")]
    Unavailable(String),
}

/// A source of raw machine signals. Adapters (MQTT/HTTP/TCP/sim) implement this
/// in M2. Unknown or malformed sources are logged and dropped — ingest must
/// never crash on bad input (§9).
#[async_trait]
pub trait SignalSource: Send + Sync {
    /// Stable identifier for this source, used in structured logs.
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy;

    #[async_trait]
    impl SignalSource for Dummy {
        fn name(&self) -> &str {
            "dummy"
        }
    }

    #[test]
    fn trait_object_is_usable() {
        let s: Box<dyn SignalSource> = Box::new(Dummy);
        assert_eq!(s.name(), "dummy");
    }
}
