//! `mes-ingest` — machine signal ingestion (§9).
//!
//! A [`SignalSource`] is a transport that yields normalized
//! [`RawSignal`](mes_client::ingest::RawSignal)s — MQTT topics, HTTP line feeds,
//! TCP streams, or the scripted [`sim`] source used in tests. Whatever the
//! transport, downstream persistence treats every signal the same and drops
//! those from an unregistered source (§9). DNC transfer events do **not** flow
//! through this path — they are owned by `mes-dnc-bridge` (§8.4, §9).

#![forbid(unsafe_code)]

pub mod sim;

use async_trait::async_trait;
use mes_client::ingest::RawSignal;

/// Errors an ingest adapter may raise while pulling signals.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("signal source unavailable: {0}")]
    Unavailable(String),
}

/// A source of raw machine signals. Adapters (MQTT/HTTP/TCP/sim) implement this.
/// Unknown or malformed input is logged and dropped by the caller — ingest must
/// never crash on bad input (§9).
#[async_trait]
pub trait SignalSource: Send + Sync {
    /// Stable identifier for this source, used in structured logs.
    fn name(&self) -> &str;

    /// Pull the next batch of available signals. An empty batch means "nothing
    /// right now", not end-of-stream.
    async fn poll(&mut self) -> Result<Vec<RawSignal>, IngestError>;
}
