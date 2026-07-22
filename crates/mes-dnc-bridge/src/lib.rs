//! `mes-dnc-bridge` — NDJSON client to the existing `dnc-daemon` (§8.4, M4).
//!
//! MES never reimplements program transfer; this crate is a *client* of the
//! daemon's local command/event surface on `127.0.0.1:8765`. The exact command
//! and event shapes are confirmed from real `dnc-daemon` source at the start of
//! M4 — **not** assumed here (§8.4, §17 Q3). M0 lands only the crate shell.

#![forbid(unsafe_code)]

/// Default local endpoint the `dnc-daemon` exposes its NDJSON surface on (§8.4).
pub const DEFAULT_DNC_DAEMON_ADDR: &str = "127.0.0.1:8765";

#[derive(Debug, thiserror::Error)]
pub enum DncBridgeError {
    #[error("dnc-daemon transport error: {0}")]
    Transport(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_addr_is_local() {
        assert!(DEFAULT_DNC_DAEMON_ADDR.starts_with("127.0.0.1"));
    }
}
