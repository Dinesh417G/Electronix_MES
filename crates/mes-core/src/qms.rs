//! QMS domain (§8, §12 M8) — pure inspection evaluation + NCR lifecycle.
//!
//! Characteristic pass/fail and the NCR disposition rules live here so the
//! auto-NCR-on-fail flow and its hold-release behaviour are deterministic and
//! unit-testable. No I/O.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Result of evaluating one measured characteristic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionOutcome {
    Pass,
    Fail,
}

impl InspectionOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
}

/// Evaluate a measurement against optional lower/upper tolerance limits
/// (inclusive). A missing bound is not enforced; no bounds at all always passes.
pub fn evaluate(
    value: Decimal,
    lower: Option<Decimal>,
    upper: Option<Decimal>,
) -> InspectionOutcome {
    if let Some(lo) = lower {
        if value < lo {
            return InspectionOutcome::Fail;
        }
    }
    if let Some(hi) = upper {
        if value > hi {
            return InspectionOutcome::Fail;
        }
    }
    InspectionOutcome::Pass
}

/// NCR lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NcrStatus {
    Open,
    Dispositioned,
    Closed,
}

impl NcrStatus {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "open" => Self::Open,
            "dispositioned" => Self::Dispositioned,
            "closed" => Self::Closed,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Dispositioned => "dispositioned",
            Self::Closed => "closed",
        }
    }
    pub fn can_transition(self, next: NcrStatus) -> bool {
        use NcrStatus::*;
        matches!(
            (self, next),
            (Open, Dispositioned) | (Dispositioned, Closed)
        )
    }
}

/// The disposition applied to a nonconformance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Rework,
    Scrap,
    UseAsIs,
    Return,
}

impl Disposition {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "rework" => Self::Rework,
            "scrap" => Self::Scrap,
            "use_as_is" => Self::UseAsIs,
            "return" => Self::Return,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rework => "rework",
            Self::Scrap => "scrap",
            Self::UseAsIs => "use_as_is",
            Self::Return => "return",
        }
    }

    /// Whether this disposition releases the quality hold so the material can
    /// move again. Rework (goes back to production) and Use-As-Is (accepted)
    /// release; Scrap and Return keep the hold in place.
    pub fn releases_hold(self) -> bool {
        matches!(self, Self::Rework | Self::UseAsIs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::prelude::FromPrimitive;

    fn d(v: f64) -> Decimal {
        Decimal::from_f64(v).unwrap()
    }

    #[test]
    fn evaluate_within_and_outside_limits() {
        let lo = Some(d(9.5));
        let hi = Some(d(10.5));
        assert_eq!(evaluate(d(10.0), lo, hi), InspectionOutcome::Pass);
        assert_eq!(evaluate(d(9.5), lo, hi), InspectionOutcome::Pass); // inclusive
        assert_eq!(evaluate(d(10.5), lo, hi), InspectionOutcome::Pass);
        assert_eq!(evaluate(d(9.4), lo, hi), InspectionOutcome::Fail);
        assert_eq!(evaluate(d(10.6), lo, hi), InspectionOutcome::Fail);
    }

    #[test]
    fn evaluate_one_sided_and_unbounded() {
        assert_eq!(
            evaluate(d(5.0), Some(d(1.0)), None),
            InspectionOutcome::Pass
        );
        assert_eq!(
            evaluate(d(0.5), Some(d(1.0)), None),
            InspectionOutcome::Fail
        );
        assert_eq!(evaluate(d(100.0), None, None), InspectionOutcome::Pass);
    }

    #[test]
    fn ncr_transitions() {
        assert!(NcrStatus::Open.can_transition(NcrStatus::Dispositioned));
        assert!(NcrStatus::Dispositioned.can_transition(NcrStatus::Closed));
        assert!(!NcrStatus::Open.can_transition(NcrStatus::Closed));
        assert!(!NcrStatus::Closed.can_transition(NcrStatus::Open));
    }

    #[test]
    fn disposition_hold_release_rules() {
        assert!(Disposition::Rework.releases_hold());
        assert!(Disposition::UseAsIs.releases_hold());
        assert!(!Disposition::Scrap.releases_hold());
        assert!(!Disposition::Return.releases_hold());
    }
}
