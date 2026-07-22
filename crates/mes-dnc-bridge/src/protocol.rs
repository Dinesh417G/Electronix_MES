//! `dnc-daemon` NDJSON protocol.
//!
//! ⚠️ **ASSUMED SHAPE — must be reconciled against real `dnc-daemon` source**
//! (§8.4, §17 Q3). The daemon repo was not available when this was written, so
//! the exact command/event names below are a documented placeholder. They are
//! deliberately confined to this one module: only the `serde` field names and
//! the `wire_*` helpers need to change once the real surface is confirmed —
//! every caller works in terms of the typed [`DncCommand`]/[`DncEvent`] enums.
//!
//! Transport is newline-delimited JSON (NDJSON) over the daemon's local socket
//! at `127.0.0.1:8765` (§4). One JSON object per line, both directions.

use serde::{Deserialize, Serialize};

/// A command MES sends to the daemon (one NDJSON line).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum DncCommand {
    /// Stage/send a program to a machine. Mirrors the daemon's
    /// trigger-program / DC1-armed-send convention (§8.4).
    SendProgram {
        /// The identifier the daemon knows the program by.
        program: String,
        /// Target machine/port, if the daemon needs it explicitly.
        #[serde(skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
    },
}

/// An event the daemon emits (one NDJSON line).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DncEvent {
    /// A previously requested transfer finished. `reference` correlates back to
    /// the value returned when the command was accepted.
    TransferCompleted { reference: String },
    /// A transfer failed.
    TransferFailed {
        reference: String,
        #[serde(default)]
        error: String,
    },
    /// The operator edited a program at the machine and sent it back. MES turns
    /// this into a **draft** program revision — never auto-promoted (§3, §8.4).
    ProgramReceived {
        program: String,
        /// Pointer to where the daemon stored the received content.
        #[serde(default)]
        content_ref: Option<String>,
    },
}

/// Serialize a command to a single NDJSON line (newline included).
pub fn wire_command(cmd: &DncCommand) -> Result<String, serde_json::Error> {
    Ok(format!("{}\n", serde_json::to_string(cmd)?))
}

/// Parse one NDJSON line into an event.
pub fn parse_event(line: &str) -> Result<DncEvent, serde_json::Error> {
    serde_json::from_str(line.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_roundtrips_ndjson() {
        let cmd = DncCommand::SendProgram {
            program: "O1234".to_string(),
            machine: Some("CNC-1".to_string()),
        };
        let line = wire_command(&cmd).unwrap();
        assert!(line.ends_with('\n'));
        let back: DncCommand = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(back, cmd);
    }

    #[test]
    fn parses_events() {
        let ev = parse_event(r#"{"event":"transfer_completed","reference":"r1"}"#).unwrap();
        assert_eq!(
            ev,
            DncEvent::TransferCompleted {
                reference: "r1".to_string()
            }
        );

        let ev =
            parse_event(r#"{"event":"program_received","program":"O9","content_ref":"blob/9"}"#)
                .unwrap();
        assert_eq!(
            ev,
            DncEvent::ProgramReceived {
                program: "O9".to_string(),
                content_ref: Some("blob/9".to_string())
            }
        );
    }
}
