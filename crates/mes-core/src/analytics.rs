//! Downtime analytics (§8.2, §12 M5) — pure Pareto + Six-Big-Losses math.
//!
//! Aggregation is done in SQL; the *ordering, share, and cumulative* maths live
//! here so they're deterministic and unit-testable against a hand-computed
//! fixture (§12 M5 acceptance). No I/O.

use serde::{Deserialize, Serialize};

/// The Six Big Losses (§8.2). Availability losses (Breakdown, SetupAdjustment)
/// and performance losses (MinorStop, ReducedSpeed) come from downtime; quality
/// losses (StartupReject, ProductionReject) come from scrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SixBigLoss {
    Breakdown,
    SetupAdjustment,
    MinorStop,
    ReducedSpeed,
    StartupReject,
    ProductionReject,
}

impl SixBigLoss {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "breakdown" => Self::Breakdown,
            "setup_adjustment" => Self::SetupAdjustment,
            "minor_stop" => Self::MinorStop,
            "reduced_speed" => Self::ReducedSpeed,
            "startup_reject" => Self::StartupReject,
            "production_reject" => Self::ProductionReject,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Breakdown => "breakdown",
            Self::SetupAdjustment => "setup_adjustment",
            Self::MinorStop => "minor_stop",
            Self::ReducedSpeed => "reduced_speed",
            Self::StartupReject => "startup_reject",
            Self::ProductionReject => "production_reject",
        }
    }

    /// Which OEE bucket this loss belongs to: "availability", "performance", or
    /// "quality" (§8.2).
    pub fn oee_bucket(self) -> &'static str {
        match self {
            Self::Breakdown | Self::SetupAdjustment => "availability",
            Self::MinorStop | Self::ReducedSpeed => "performance",
            Self::StartupReject | Self::ProductionReject => "quality",
        }
    }
}

/// One aggregated category feeding a Pareto chart, before ranking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParetoInput {
    pub key: String,
    pub label: String,
    pub seconds: i64,
}

/// A ranked Pareto row with share and running cumulative share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParetoRow {
    pub key: String,
    pub label: String,
    pub seconds: i64,
    /// This row's share of the total, 0–100.
    pub pct: f64,
    /// Running cumulative share through this row, 0–100.
    pub cumulative_pct: f64,
}

/// Rank categories by descending magnitude and compute share + cumulative share
/// (the classic Pareto ordering). Ties break by `key` for determinism. Rows
/// with zero total are dropped. An empty or all-zero input yields no rows.
pub fn pareto(mut inputs: Vec<ParetoInput>) -> Vec<ParetoRow> {
    inputs.retain(|i| i.seconds > 0);
    inputs.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.key.cmp(&b.key)));

    let total: i64 = inputs.iter().map(|i| i.seconds).sum();
    if total == 0 {
        return Vec::new();
    }

    let mut running = 0i64;
    inputs
        .into_iter()
        .map(|i| {
            running += i.seconds;
            ParetoRow {
                pct: (i.seconds as f64) * 100.0 / (total as f64),
                cumulative_pct: (running as f64) * 100.0 / (total as f64),
                key: i.key,
                label: i.label,
                seconds: i.seconds,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inp(key: &str, seconds: i64) -> ParetoInput {
        ParetoInput {
            key: key.to_string(),
            label: key.to_string(),
            seconds,
        }
    }

    #[test]
    fn pareto_orders_descending_with_cumulative() {
        // Hand-computed fixture: totals 50/30/20 → 100.
        let rows = pareto(vec![inp("b", 30), inp("a", 50), inp("c", 20)]);
        let keys: Vec<_> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["a", "b", "c"], "sorted by magnitude desc");
        assert_eq!(rows[0].seconds, 50);
        assert!((rows[0].pct - 50.0).abs() < 1e-9);
        assert!((rows[0].cumulative_pct - 50.0).abs() < 1e-9);
        assert!((rows[1].cumulative_pct - 80.0).abs() < 1e-9);
        assert!((rows[2].cumulative_pct - 100.0).abs() < 1e-9);
    }

    #[test]
    fn pareto_breaks_ties_by_key() {
        let rows = pareto(vec![inp("z", 10), inp("a", 10)]);
        assert_eq!(rows[0].key, "a");
        assert_eq!(rows[1].key, "z");
    }

    #[test]
    fn pareto_drops_zero_and_empty() {
        assert!(pareto(vec![]).is_empty());
        assert!(pareto(vec![inp("a", 0)]).is_empty());
        let rows = pareto(vec![inp("a", 5), inp("b", 0)]);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn six_big_loss_buckets() {
        assert_eq!(SixBigLoss::Breakdown.oee_bucket(), "availability");
        assert_eq!(SixBigLoss::MinorStop.oee_bucket(), "performance");
        assert_eq!(SixBigLoss::ProductionReject.oee_bucket(), "quality");
        assert_eq!(SixBigLoss::parse("breakdown"), Some(SixBigLoss::Breakdown));
        assert_eq!(SixBigLoss::parse("nope"), None);
    }
}
