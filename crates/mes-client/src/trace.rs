//! Traceability DTOs (§10 `/v1/trace`).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LotInput {
    pub lot_no: String,
    pub part_id: String,
    #[schema(value_type = String)]
    pub qty: Option<Decimal>,
    pub uom: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Lot {
    pub id: String,
    pub lot_no: String,
    pub part_id: String,
    #[schema(value_type = String)]
    pub qty: Decimal,
    pub uom: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SerialInput {
    pub serial_no: String,
    pub part_id: String,
    pub lot_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Serial {
    pub id: String,
    pub serial_no: String,
    pub part_id: String,
    pub lot_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A genealogy edge: `parent` (assembly/output) consumed `child` (component).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GenealogyEdgeInput {
    pub parent_type: String,
    pub parent_id: String,
    pub child_type: String,
    pub child_id: String,
    #[schema(value_type = Option<String>)]
    pub qty: Option<Decimal>,
}

/// A node in a trace result, with its depth from the queried root.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TraceNode {
    pub entity_type: String,
    pub entity_id: String,
    /// Human-readable lot_no / serial_no, if the entity exists.
    pub ref_no: Option<String>,
    pub depth: i32,
}

/// Issue material against a work order operation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IssueMaterialInput {
    pub lot_id: Option<String>,
    pub serial_id: Option<String>,
    #[schema(value_type = String)]
    pub qty: Decimal,
    pub wo_operation_id: Option<String>,
}

/// Place a hold on an entity.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HoldInput {
    pub entity_type: String,
    pub entity_id: String,
    pub reason: Option<String>,
}

/// Result of parsing a barcode.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BarcodeParsed {
    pub type_code: String,
    pub id: String,
}
