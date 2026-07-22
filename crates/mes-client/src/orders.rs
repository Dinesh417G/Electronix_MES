//! Work-order DTOs (§10 `/v1/orders`).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One operation within a work order, as supplied on creation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WoOperationInput {
    pub op_no: i32,
    pub routing_op_id: Option<String>,
    pub work_center_id: Option<String>,
}

/// Create-work-order request. Operations are created together with the order.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkOrderInput {
    pub wo_number: String,
    pub part_id: String,
    pub routing_id: Option<String>,
    #[schema(value_type = String)]
    pub qty_ordered: Decimal,
    pub priority: Option<i32>,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    #[serde(default)]
    pub operations: Vec<WoOperationInput>,
}

/// A work order.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkOrder {
    pub id: String,
    pub wo_number: String,
    pub part_id: String,
    pub routing_id: Option<String>,
    #[schema(value_type = String)]
    pub qty_ordered: Decimal,
    pub priority: i32,
    pub status: String,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An operation within a work order.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WoOperation {
    pub id: String,
    pub work_order_id: String,
    pub routing_op_id: Option<String>,
    pub op_no: i32,
    pub work_center_id: Option<String>,
    pub status: String,
    pub qty_good: i32,
    pub qty_scrap: i32,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A work order with its operations.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkOrderDetail {
    #[serde(flatten)]
    pub work_order: WorkOrder,
    pub operations: Vec<WoOperation>,
}
