//! CMMS DTOs (§10 `/v1/cmms`) — PM schedules, maintenance WOs, spares, and
//! procurement requests (§7, §12 M9).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ---- PM schedules --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PmScheduleInput {
    pub work_center_id: String,
    pub name: String,
    /// `calendar` or `usage_hours`.
    pub trigger_type: String,
    /// Days for calendar; run-hours for usage_hours.
    #[schema(value_type = String)]
    pub interval_value: Decimal,
    pub checklist_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PmSchedule {
    pub id: String,
    pub work_center_id: String,
    pub name: String,
    pub trigger_type: String,
    #[schema(value_type = String)]
    pub interval_value: Decimal,
    pub last_done_at: Option<DateTime<Utc>>,
    pub next_due_at: Option<DateTime<Utc>>,
    #[schema(value_type = Option<String>)]
    pub last_done_usage_h: Option<Decimal>,
    #[schema(value_type = Option<String>)]
    pub next_due_usage_h: Option<Decimal>,
    pub checklist_ref: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A PM schedule that is currently due, with the current run-hours that were
/// used to judge a usage-hours trigger (helpful context on the due list).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PmDue {
    pub schedule: PmSchedule,
    #[schema(value_type = String)]
    pub current_usage_h: Decimal,
}

// ---- Maintenance work orders ---------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MaintenanceWoInput {
    pub work_center_id: String,
    pub pm_schedule_id: Option<String>,
    /// `PM`, `Corrective`, or `Breakdown`.
    pub wo_type: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MaintenanceWo {
    pub id: String,
    pub work_center_id: String,
    pub pm_schedule_id: Option<String>,
    pub wo_type: String,
    pub status: String,
    pub technician_id: Option<String>,
    pub failure_code: Option<String>,
    pub notes: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Advance a maintenance WO to the next lifecycle status.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MaintenanceTransitionInput {
    /// Target status: `scheduled` | `in_progress` | `completed` | `verified`.
    pub status: String,
    pub technician_id: Option<String>,
    pub failure_code: Option<String>,
}

// ---- Spares --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SparePartInput {
    pub code: String,
    pub name: String,
    pub uom: Option<String>,
    #[schema(value_type = Option<String>)]
    pub reorder_point: Option<Decimal>,
    #[schema(value_type = Option<String>)]
    pub reorder_qty: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SparePart {
    pub id: String,
    pub code: String,
    pub name: String,
    pub uom: String,
    #[schema(value_type = String)]
    pub reorder_point: Decimal,
    #[schema(value_type = String)]
    pub reorder_qty: Decimal,
    /// Current stock, derived by summing the txn ledger (§7).
    #[schema(value_type = String)]
    pub stock: Decimal,
}

/// Record a spare movement. `qty` is always given as a positive magnitude; the
/// server applies the sign from `txn_type` (issue decreases stock).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SpareTxnInput {
    pub spare_part_id: String,
    pub maintenance_wo_id: Option<String>,
    /// `issue` | `receive` | `adjust`.
    pub txn_type: String,
    #[schema(value_type = String)]
    pub qty: Decimal,
}

/// Result of a spare txn: the new derived stock plus any auto-raised
/// reorder-point procurement request (§7).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SpareTxnResponse {
    pub txn_id: String,
    #[schema(value_type = String)]
    pub stock: Decimal,
    pub procurement_request: Option<ProcurementRequest>,
}

// ---- Procurement requests ------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProcurementRequestInput {
    pub spare_part_id: String,
    #[schema(value_type = String)]
    pub qty_requested: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProcurementRequest {
    pub id: String,
    pub spare_part_id: String,
    #[schema(value_type = String)]
    pub qty_requested: Decimal,
    pub reason: String,
    pub status: String,
    pub erp_reference: Option<String>,
    pub pushed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
