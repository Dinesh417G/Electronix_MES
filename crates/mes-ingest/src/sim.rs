//! Scripted simulation source (§13) — deterministic signals for tests and demos.
//!
//! Holds a pre-built list of [`RawSignal`]s and hands them out on the first
//! `poll`, then reports empty batches. Used by the M2 end-to-end test and the
//! `machine-sim` tool (built out at M2/M4).

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use mes_client::ingest::{RawSignal, SignalEvent};

use crate::{IngestError, SignalSource};

/// A source that replays a fixed script of signals once.
pub struct SimSource {
    name: String,
    pending: Vec<RawSignal>,
    drained: bool,
}

impl SimSource {
    pub fn new(name: impl Into<String>, signals: Vec<RawSignal>) -> Self {
        Self {
            name: name.into(),
            pending: signals,
            drained: false,
        }
    }

    /// Build a run of cycle pulses every `step_secs` over `[from, to]` inclusive,
    /// all attributed to `source_key`.
    pub fn cycle_run(
        source_key: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        step_secs: i64,
    ) -> Vec<RawSignal> {
        let mut out = Vec::new();
        let mut t = from;
        while t <= to {
            out.push(RawSignal {
                source_key: source_key.to_string(),
                ts: t,
                event: SignalEvent::Cycle,
            });
            t += Duration::seconds(step_secs);
        }
        out
    }
}

#[async_trait]
impl SignalSource for SimSource {
    fn name(&self) -> &str {
        &self.name
    }

    async fn poll(&mut self) -> Result<Vec<RawSignal>, IngestError> {
        if self.drained {
            return Ok(Vec::new());
        }
        self.drained = true;
        Ok(std::mem::take(&mut self.pending))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, h, m, s).single().unwrap()
    }

    #[tokio::test]
    async fn sim_drains_once() {
        let signals = SimSource::cycle_run("m1", ts(10, 0, 0), ts(10, 1, 0), 30);
        assert_eq!(signals.len(), 3); // 0s, 30s, 60s
        let mut src = SimSource::new("sim", signals);
        let first = src.poll().await.unwrap();
        assert_eq!(first.len(), 3);
        let second = src.poll().await.unwrap();
        assert!(second.is_empty(), "source drains after first poll");
    }
}
