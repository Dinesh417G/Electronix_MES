//! `mes-dnc-bridge` — transport to the existing `dnc-daemon` (§8.4, M4).
//!
//! MES never reimplements program transfer; this crate is a *client* of the
//! daemon's local NDJSON surface on `127.0.0.1:8765`. It provides the typed
//! [`protocol`] (whose wire shape is an assumption to reconcile with real
//! `dnc-daemon` source, §17 Q3) and the swappable [`DncDaemon`] command channel.
//! The business flow (writing `dnc_transfer_events`, creating draft
//! `program_revisions`, WS notifications) lives in `mes-edge`, which depends on
//! the DB and WS bus — this crate stays a pure, dependency-light transport.

#![forbid(unsafe_code)]

pub mod client;
pub mod protocol;

pub use client::{DisconnectedDaemon, DncDaemon, TcpDncClient, VirtualDaemon};
pub use protocol::{parse_event, wire_command, DncCommand, DncEvent};

/// Default local endpoint the `dnc-daemon` exposes its NDJSON surface on (§8.4).
pub const DEFAULT_DNC_DAEMON_ADDR: &str = "127.0.0.1:8765";

#[derive(Debug, thiserror::Error)]
pub enum DncBridgeError {
    #[error("dnc-daemon transport error: {0}")]
    Transport(String),
    #[error("dnc-daemon protocol error: {0}")]
    Protocol(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_addr_is_local() {
        assert!(DEFAULT_DNC_DAEMON_ADDR.starts_with("127.0.0.1"));
    }
}
