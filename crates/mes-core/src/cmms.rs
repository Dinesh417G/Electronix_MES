//! CMMS domain (§7, §12 M9) — PM-due calculation and the maintenance-WO /
//! procurement lifecycles. Pure, no I/O.
//!
//! Preventive-maintenance schedules trigger either on a calendar interval (days)
//! or on cumulative machine run-hours. The run-hours themselves come from the
//! existing `machine_states` RUNNING intervals (§7 — no new raw data); this
//! module only decides, given a schedule's next-due marker and the current
//! clock/run-hours, whether it is due. Keeping that decision here makes the
//! trigger deterministic and unit-testable (§13).

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// How a PM schedule is triggered (§7 `pm_schedules.trigger_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PmTrigger {
    /// Every N days of wall-clock time since the last service.
    Calendar,
    /// Every N cumulative RUNNING hours since the last service.
    UsageHours,
}

impl PmTrigger {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "calendar" => Self::Calendar,
            "usage_hours" => Self::UsageHours,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Calendar => "calendar",
            Self::UsageHours => "usage_hours",
        }
    }
}

/// Next calendar due time = last service + `interval_days`.
pub fn calendar_next_due(last_done: DateTime<Utc>, interval_days: i64) -> DateTime<Utc> {
    last_done + Duration::days(interval_days)
}

/// A calendar schedule is due once `now` reaches its next-due time.
pub fn calendar_is_due(next_due: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now >= next_due
}

/// Next usage-hours due marker = run-hours at last service + `interval_hours`.
pub fn usage_next_due(last_done_usage_h: Decimal, interval_hours: Decimal) -> Decimal {
    last_done_usage_h + interval_hours
}

/// A usage-hours schedule is due once cumulative run-hours reach its marker.
pub fn usage_is_due(next_due_usage_h: Decimal, current_usage_h: Decimal) -> bool {
    current_usage_h >= next_due_usage_h
}

/// Category of a maintenance work order (§7 `maintenance_work_orders.type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaintenanceType {
    /// Planned preventive maintenance (raised off a `pm_schedule`).
    Pm,
    /// Reactive repair of a known-but-not-stopping fault.
    Corrective,
    /// Emergency repair of a machine that has stopped.
    Breakdown,
}

impl MaintenanceType {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "PM" => Self::Pm,
            "Corrective" => Self::Corrective,
            "Breakdown" => Self::Breakdown,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pm => "PM",
            Self::Corrective => "Corrective",
            Self::Breakdown => "Breakdown",
        }
    }
}

/// Maintenance-WO lifecycle (§7). A closed WO *is* the maintenance history —
/// there is no separate history table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceStatus {
    Requested,
    Scheduled,
    InProgress,
    Completed,
    Verified,
}

impl MaintenanceStatus {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "requested" => Self::Requested,
            "scheduled" => Self::Scheduled,
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            "verified" => Self::Verified,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Scheduled => "scheduled",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Verified => "verified",
        }
    }

    /// The lifecycle advances one step at a time and never moves backwards.
    pub fn can_transition(self, next: MaintenanceStatus) -> bool {
        use MaintenanceStatus::*;
        matches!(
            (self, next),
            (Requested, Scheduled)
                | (Scheduled, InProgress)
                | (InProgress, Completed)
                | (Completed, Verified)
        )
    }
}

/// Procurement-request lifecycle (§7). In v1 MES only *raises* requests — the
/// PO/vendor lifecycle stays in ERP (§3). The status therefore caps at
/// `Requested` until M10 wires the ERP push (`SentToErp` → `Fulfilled`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcurementStatus {
    Requested,
    SentToErp,
    Fulfilled,
}

impl ProcurementStatus {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "requested" => Self::Requested,
            "sent_to_erp" => Self::SentToErp,
            "fulfilled" => Self::Fulfilled,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::SentToErp => "sent_to_erp",
            Self::Fulfilled => "fulfilled",
        }
    }
    pub fn can_transition(self, next: ProcurementStatus) -> bool {
        use ProcurementStatus::*;
        matches!(
            (self, next),
            (Requested, SentToErp) | (SentToErp, Fulfilled)
        )
    }
}

/// Why a procurement request was raised (§7 `procurement_requests.reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcurementReason {
    /// Stock fell to or below the spare's reorder point (auto).
    ReorderPoint,
    /// Raised by hand from the CMMS console.
    Manual,
}

impl ProcurementReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReorderPoint => "reorder_point",
            Self::Manual => "manual",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rust_decimal::prelude::FromPrimitive;

    fn d(v: f64) -> Decimal {
        Decimal::from_f64(v).unwrap()
    }

    fn t(y: i32, m: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, day, 0, 0, 0).unwrap()
    }

    #[test]
    fn calendar_due_at_and_after_interval() {
        let last = t(2026, 1, 1);
        let next = calendar_next_due(last, 30);
        assert_eq!(next, t(2026, 1, 31));
        assert!(!calendar_is_due(next, t(2026, 1, 30))); // one day early
        assert!(calendar_is_due(next, t(2026, 1, 31))); // exactly due
        assert!(calendar_is_due(next, t(2026, 2, 5))); // overdue
    }

    #[test]
    fn usage_due_off_run_hours() {
        // Serviced at 100 run-hours, service every 250 → next due at 350h.
        let next = usage_next_due(d(100.0), d(250.0));
        assert_eq!(next, d(350.0));
        assert!(!usage_is_due(next, d(349.9))); // not yet
        assert!(usage_is_due(next, d(350.0))); // exactly
        assert!(usage_is_due(next, d(400.0))); // overdue
    }

    #[test]
    fn maintenance_lifecycle_is_forward_only() {
        use MaintenanceStatus::*;
        assert!(Requested.can_transition(Scheduled));
        assert!(Scheduled.can_transition(InProgress));
        assert!(InProgress.can_transition(Completed));
        assert!(Completed.can_transition(Verified));
        // No skipping and no going backwards.
        assert!(!Requested.can_transition(InProgress));
        assert!(!Requested.can_transition(Completed));
        assert!(!Completed.can_transition(InProgress));
        assert!(!Verified.can_transition(Completed));
    }

    #[test]
    fn procurement_status_advances_but_caps_conceptually_at_requested_in_v1() {
        use ProcurementStatus::*;
        assert!(Requested.can_transition(SentToErp));
        assert!(SentToErp.can_transition(Fulfilled));
        assert!(!Requested.can_transition(Fulfilled));
        assert!(!Fulfilled.can_transition(Requested));
    }

    #[test]
    fn round_trip_string_forms() {
        assert_eq!(PmTrigger::parse("usage_hours"), Some(PmTrigger::UsageHours));
        assert_eq!(PmTrigger::UsageHours.as_str(), "usage_hours");
        assert_eq!(MaintenanceType::parse("PM"), Some(MaintenanceType::Pm));
        assert_eq!(
            MaintenanceStatus::parse("in_progress"),
            Some(MaintenanceStatus::InProgress)
        );
        assert_eq!(PmTrigger::parse("nope"), None);
    }
}
