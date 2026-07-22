//! Daemon transport: the [`DncDaemon`] command channel and its implementations.
//!
//! Inbound events are handled by the caller (mes-edge orchestration) — this
//! layer is a thin, swappable transport so the business flow can run against a
//! [`VirtualDaemon`] in tests (§13, "never real CNC hardware") and a real TCP
//! client in production without changing a line of orchestration code.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use crate::protocol::{wire_command, DncCommand};
use crate::DncBridgeError;

/// The command side of the daemon: send a command, get back a correlation
/// reference the daemon will echo in its completion event.
#[async_trait]
pub trait DncDaemon: Send + Sync {
    async fn send(&self, cmd: DncCommand) -> Result<String, DncBridgeError>;
}

/// In-memory daemon for tests and offline demos. Records every command and
/// hands back a deterministic reference (`sim-N`).
#[derive(Default)]
pub struct VirtualDaemon {
    sent: Mutex<Vec<DncCommand>>,
    counter: AtomicU64,
}

impl VirtualDaemon {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of commands sent so far.
    pub fn sent(&self) -> Vec<DncCommand> {
        self.sent.lock().expect("lock").clone()
    }
}

#[async_trait]
impl DncDaemon for VirtualDaemon {
    async fn send(&self, cmd: DncCommand) -> Result<String, DncBridgeError> {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        self.sent.lock().expect("lock").push(cmd);
        Ok(format!("sim-{n}"))
    }
}

/// A daemon that is not connected — the default in a deployment where no CNC /
/// dnc-daemon is present. Every send fails cleanly so orchestration degrades
/// instead of panicking.
pub struct DisconnectedDaemon;

#[async_trait]
impl DncDaemon for DisconnectedDaemon {
    async fn send(&self, _cmd: DncCommand) -> Result<String, DncBridgeError> {
        Err(DncBridgeError::Transport(
            "dnc-daemon not configured".into(),
        ))
    }
}

/// Real NDJSON client over the daemon's local TCP socket (§4, §8.4).
///
/// ⚠️ The acknowledgement shape (how the daemon returns its correlation
/// reference) is part of the protocol that must be confirmed from real
/// `dnc-daemon` source (§17 Q3); until then this generates a local reference.
pub struct TcpDncClient {
    addr: String,
    counter: AtomicU64,
}

impl TcpDncClient {
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            counter: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl DncDaemon for TcpDncClient {
    async fn send(&self, cmd: DncCommand) -> Result<String, DncBridgeError> {
        let line = wire_command(&cmd).map_err(|e| DncBridgeError::Protocol(e.to_string()))?;
        let mut stream = TcpStream::connect(&self.addr)
            .await
            .map_err(|e| DncBridgeError::Transport(e.to_string()))?;
        stream
            .write_all(line.as_bytes())
            .await
            .map_err(|e| DncBridgeError::Transport(e.to_string()))?;
        stream
            .flush()
            .await
            .map_err(|e| DncBridgeError::Transport(e.to_string()))?;
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        Ok(format!("tcp-{n}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn virtual_daemon_records_and_refs() {
        let d = VirtualDaemon::new();
        let r0 = d
            .send(DncCommand::SendProgram {
                program: "O1".into(),
                machine: None,
            })
            .await
            .unwrap();
        assert_eq!(r0, "sim-0");
        assert_eq!(d.sent().len(), 1);
    }

    #[tokio::test]
    async fn disconnected_daemon_errors() {
        let d = DisconnectedDaemon;
        let r = d
            .send(DncCommand::SendProgram {
                program: "O1".into(),
                machine: None,
            })
            .await;
        assert!(r.is_err());
    }
}
