//! Ingestion DTOs (§9, §10 `/v1/ingest`).
//!
//! Devices post batches of [`RawSignal`]. Each carries the `source_key` its
//! sender is registered under; signals from an unknown source are dropped (§9).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The kind of signal, tagged by `type` on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalEvent {
    /// One production cycle completed — drives the state machine.
    Cycle,
    /// The machine is alive but not necessarily producing.
    Heartbeat,
    /// A production tally (good/scrap) at this instant.
    Count {
        #[serde(default)]
        good: i32,
        #[serde(default)]
        scrap: i32,
    },
}

impl SignalEvent {
    /// The `machine_events.event_type` string for this signal.
    pub fn event_type(&self) -> &'static str {
        match self {
            SignalEvent::Cycle => "cycle",
            SignalEvent::Heartbeat => "heartbeat",
            SignalEvent::Count { .. } => "count",
        }
    }
}

/// A single raw signal from a device.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RawSignal {
    /// Registered key identifying the sending source (§9).
    pub source_key: String,
    /// When the signal occurred (device clock, UTC).
    pub ts: DateTime<Utc>,
    #[serde(flatten)]
    pub event: SignalEvent,
}

/// Outcome of an ingest batch.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestResult {
    /// Number of signals persisted.
    pub accepted: usize,
    /// Number dropped because their source was unknown or disabled (§9).
    pub dropped: usize,
}

/// Request to (re)derive machine states for a work center over a window.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecomputeRequest {
    pub work_center_id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Result of a recompute.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecomputeResult {
    /// Number of state intervals written.
    pub states: usize,
    /// Number of (unclassified) downtime events written.
    pub downtime: usize,
}
