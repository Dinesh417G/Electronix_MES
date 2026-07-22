//! Machine state machine (§8.1) — pure, deterministic, I/O-free.
//!
//! A machine emits **cycle pulses** while it produces parts. This engine turns a
//! time-ordered pulse stream over a window into a sequence of non-overlapping
//! [`StateInterval`]s (Running / MicroStop / Down / PlannedStop) and derives an
//! unclassified [`DowntimeEvent`] for each stop. Reasons are attached later by an
//! operator (M3/M5) — the engine only *detects* and *classifies by duration*.
//!
//! ## Rules (thresholds are configurable; defaults documented on [`StateConfig`])
//!
//! - **Debounce**: pulses within `debounce` of the previously kept pulse are
//!   duplicates and dropped, so contact bounce / double-reports don't create
//!   spurious sub-second intervals.
//! - **Running**: while consecutive pulses are no more than `micro_stop_after`
//!   apart, the machine is Running — that gap is normal cycle-time variation.
//! - **Stop**: a gap larger than `micro_stop_after` is a stop, beginning at the
//!   last pulse. A stop no longer than `down_after` is a **MicroStop** (short
//!   stop); a longer one is **Down**.
//! - **Planned stops**: any span covered by a planned-stop interval is overlaid
//!   as **PlannedStop**, overriding whatever the pulse stream implied there.
//! - **Shift boundaries**: [`split_at_boundaries`] closes an interval exactly at
//!   a boundary and reopens the same state after it, so per-shift rollups are
//!   clean.
//!
//! The v1 spec left exact numbers open; the defaults here are the contract M2
//! tests pin, and they're overridable per work center when that wiring lands.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::id::new_id;

/// Discrete machine state over an interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineState {
    Running,
    MicroStop,
    Down,
    PlannedStop,
}

impl MachineState {
    /// Whether this state is a stop (i.e. not producing).
    pub fn is_stop(self) -> bool {
        !matches!(self, MachineState::Running)
    }
}

/// Thresholds governing state transitions.
#[derive(Debug, Clone, Copy)]
pub struct StateConfig {
    /// Max gap between pulses still counted as Running (normal cycle variation).
    pub micro_stop_after: Duration,
    /// A stop longer than this is Down; at or under it is MicroStop.
    pub down_after: Duration,
    /// Pulses within this of the previous kept pulse are dropped as duplicates.
    pub debounce: Duration,
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            micro_stop_after: Duration::seconds(60),
            down_after: Duration::seconds(300), // 5 minutes
            debounce: Duration::seconds(2),
        }
    }
}

/// A resolved, non-overlapping span of a single state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateInterval {
    pub state: MachineState,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl StateInterval {
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

/// A planned stop (from `planned_stops`) overlaid onto the timeline.
#[derive(Debug, Clone, Copy)]
pub struct PlannedInterval {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// An unclassified downtime occurrence derived from a stop interval. `reason`
/// stays `None` until an operator classifies it (M3/M5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DowntimeEvent {
    pub id: String,
    pub state: MachineState,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Classify a stop's duration into MicroStop vs Down.
fn classify_stop(cfg: &StateConfig, span: Duration) -> MachineState {
    if span <= cfg.down_after {
        MachineState::MicroStop
    } else {
        MachineState::Down
    }
}

/// Push a Running interval, skipping zero/negative-length spans.
fn push_running(out: &mut Vec<StateInterval>, start: DateTime<Utc>, end: DateTime<Utc>) {
    if end > start {
        out.push(StateInterval {
            state: MachineState::Running,
            start,
            end,
        });
    }
}

/// Drop pulses that fall within `debounce` of the previously kept pulse. Input
/// is assumed sorted ascending; out-of-order pulses are ignored.
fn debounce(pulses: &[DateTime<Utc>], min_gap: Duration) -> Vec<DateTime<Utc>> {
    let mut kept: Vec<DateTime<Utc>> = Vec::with_capacity(pulses.len());
    for &p in pulses {
        match kept.last() {
            Some(&last) if p - last < min_gap => {}
            Some(&last) if p < last => {} // out of order; skip
            _ => kept.push(p),
        }
    }
    kept
}

/// Compute the raw state timeline for a pulse stream over `[window_start,
/// window_end]`, before any planned-stop overlay or shift-boundary splitting.
pub fn compute_states(
    cfg: &StateConfig,
    pulses: &[DateTime<Utc>],
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Vec<StateInterval> {
    let mut out: Vec<StateInterval> = Vec::new();
    if window_end <= window_start {
        return out;
    }

    let pulses = debounce(pulses, cfg.debounce);

    // No pulses at all: the whole window is one stop.
    let Some(&first) = pulses.first() else {
        out.push(StateInterval {
            state: classify_stop(cfg, window_end - window_start),
            start: window_start,
            end: window_end,
        });
        return out;
    };

    // Leading span before the first pulse.
    let mut run_start = if first - window_start <= cfg.micro_stop_after {
        window_start
    } else {
        out.push(StateInterval {
            state: classify_stop(cfg, first - window_start),
            start: window_start,
            end: first,
        });
        first
    };

    // Walk the pulses, coalescing running spans and cutting stops on big gaps.
    let mut last = first;
    for &p in pulses.iter().skip(1) {
        let gap = p - last;
        if gap <= cfg.micro_stop_after {
            last = p;
            continue;
        }
        push_running(&mut out, run_start, last);
        out.push(StateInterval {
            state: classify_stop(cfg, gap),
            start: last,
            end: p,
        });
        run_start = p;
        last = p;
    }

    // Trailing span after the last pulse.
    if window_end - last <= cfg.micro_stop_after {
        push_running(&mut out, run_start, window_end);
    } else {
        push_running(&mut out, run_start, last);
        out.push(StateInterval {
            state: classify_stop(cfg, window_end - last),
            start: last,
            end: window_end,
        });
    }

    out
}

/// Overlay planned stops onto a computed timeline: any sub-span of an interval
/// covered by a planned interval becomes [`MachineState::PlannedStop`].
pub fn apply_planned_stops(
    intervals: &[StateInterval],
    planned: &[PlannedInterval],
) -> Vec<StateInterval> {
    let mut out = Vec::new();
    for iv in intervals {
        // Collect planned overlaps clipped to this interval, sorted by start.
        let mut cuts: Vec<(DateTime<Utc>, DateTime<Utc>)> = planned
            .iter()
            .filter_map(|p| {
                let s = p.start.max(iv.start);
                let e = p.end.min(iv.end);
                (s < e).then_some((s, e))
            })
            .collect();
        cuts.sort_by_key(|(s, _)| *s);

        let mut cursor = iv.start;
        for (s, e) in cuts {
            if s > cursor {
                out.push(StateInterval {
                    state: iv.state,
                    start: cursor,
                    end: s,
                });
            }
            // Merge with a previous adjacent PlannedStop if contiguous.
            if let Some(last) = out.last_mut() {
                if last.state == MachineState::PlannedStop && last.end >= s.max(cursor) {
                    last.end = last.end.max(e);
                    cursor = last.end;
                    continue;
                }
            }
            out.push(StateInterval {
                state: MachineState::PlannedStop,
                start: cursor.max(s),
                end: e,
            });
            cursor = e;
        }
        if cursor < iv.end {
            out.push(StateInterval {
                state: iv.state,
                start: cursor,
                end: iv.end,
            });
        }
    }
    out
}

/// Split intervals at the given boundary timestamps (e.g. shift changes), so no
/// interval straddles a boundary. Boundaries outside an interval are ignored.
pub fn split_at_boundaries(
    intervals: &[StateInterval],
    boundaries: &[DateTime<Utc>],
) -> Vec<StateInterval> {
    let mut out = Vec::new();
    for iv in intervals {
        let mut cursor = iv.start;
        let mut inner: Vec<DateTime<Utc>> = boundaries
            .iter()
            .copied()
            .filter(|&b| b > iv.start && b < iv.end)
            .collect();
        inner.sort();
        for b in inner {
            out.push(StateInterval {
                state: iv.state,
                start: cursor,
                end: b,
            });
            cursor = b;
        }
        out.push(StateInterval {
            state: iv.state,
            start: cursor,
            end: iv.end,
        });
    }
    out
}

/// Derive an unclassified [`DowntimeEvent`] for every stop interval.
pub fn downtime_events(intervals: &[StateInterval]) -> Vec<DowntimeEvent> {
    intervals
        .iter()
        .filter(|iv| iv.state.is_stop() && iv.state != MachineState::PlannedStop)
        .map(|iv| DowntimeEvent {
            id: new_id(),
            state: iv.state,
            start: iv.start,
            end: iv.end,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, h, m, s).single().unwrap()
    }

    /// Emit a pulse every `step` seconds over `[from, to]` inclusive of `from`.
    fn pulses_every(from: DateTime<Utc>, to: DateTime<Utc>, step: i64) -> Vec<DateTime<Utc>> {
        let mut v = Vec::new();
        let mut t = from;
        while t <= to {
            v.push(t);
            t += Duration::seconds(step);
        }
        v
    }

    #[test]
    fn debounce_drops_close_duplicates() {
        let cfg = StateConfig::default();
        let base = ts(10, 0, 0);
        let raw = vec![
            base,
            base + Duration::milliseconds(500),
            base + Duration::seconds(3),
        ];
        let kept = debounce(&raw, cfg.debounce);
        assert_eq!(kept.len(), 2, "the 0.5s duplicate is dropped");
    }

    #[test]
    fn no_pulses_is_one_stop() {
        let cfg = StateConfig::default();
        let out = compute_states(&cfg, &[], ts(10, 0, 0), ts(10, 2, 0));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].state, MachineState::MicroStop); // 2min <= 5min
    }

    #[test]
    fn classify_by_duration() {
        let cfg = StateConfig::default();
        assert_eq!(
            classify_stop(&cfg, Duration::seconds(120)),
            MachineState::MicroStop
        );
        assert_eq!(
            classify_stop(&cfg, Duration::seconds(600)),
            MachineState::Down
        );
    }

    #[test]
    fn planned_stop_overrides_running() {
        let running = vec![StateInterval {
            state: MachineState::Running,
            start: ts(10, 0, 0),
            end: ts(10, 30, 0),
        }];
        let planned = vec![PlannedInterval {
            start: ts(10, 10, 0),
            end: ts(10, 15, 0),
        }];
        let out = apply_planned_stops(&running, &planned);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].state, MachineState::Running);
        assert_eq!(out[1].state, MachineState::PlannedStop);
        assert_eq!(out[1].start, ts(10, 10, 0));
        assert_eq!(out[1].end, ts(10, 15, 0));
        assert_eq!(out[2].state, MachineState::Running);
    }

    #[test]
    fn split_at_shift_boundary() {
        let running = vec![StateInterval {
            state: MachineState::Running,
            start: ts(5, 30, 0),
            end: ts(6, 30, 0),
        }];
        let out = split_at_boundaries(&running, &[ts(6, 0, 0)]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].end, ts(6, 0, 0));
        assert_eq!(out[1].start, ts(6, 0, 0));
        assert_eq!(out[0].state, out[1].state);
    }

    /// The M2 acceptance scenario: run → micro-stop → down → run over one hour.
    #[test]
    fn scripted_hour_run_microstop_down_run() {
        let cfg = StateConfig::default();
        let mut pulses = Vec::new();
        // RUN 10:00:00–10:20:00, pulse every 30s (<=60s ⇒ running).
        pulses.extend(pulses_every(ts(10, 0, 0), ts(10, 20, 0), 30));
        // MICRO_STOP gap 10:20:00–10:23:00 (3min ≤ 5min).
        // RUN 10:23:00–10:40:00.
        pulses.extend(pulses_every(ts(10, 23, 0), ts(10, 40, 0), 30));
        // DOWN gap 10:40:00–10:50:00 (10min > 5min).
        // RUN 10:50:00–11:00:00.
        pulses.extend(pulses_every(ts(10, 50, 0), ts(11, 0, 0), 30));

        let out = compute_states(&cfg, &pulses, ts(10, 0, 0), ts(11, 0, 0));

        let expected = vec![
            (MachineState::Running, ts(10, 0, 0), ts(10, 20, 0)),
            (MachineState::MicroStop, ts(10, 20, 0), ts(10, 23, 0)),
            (MachineState::Running, ts(10, 23, 0), ts(10, 40, 0)),
            (MachineState::Down, ts(10, 40, 0), ts(10, 50, 0)),
            (MachineState::Running, ts(10, 50, 0), ts(11, 0, 0)),
        ];
        let got: Vec<_> = out.iter().map(|i| (i.state, i.start, i.end)).collect();
        assert_eq!(got, expected);

        // One MicroStop + one Down ⇒ two downtime events (PlannedStop excluded).
        let dts = downtime_events(&out);
        assert_eq!(dts.len(), 2);
        assert_eq!(dts[0].state, MachineState::MicroStop);
        assert_eq!(dts[1].state, MachineState::Down);
    }
}
