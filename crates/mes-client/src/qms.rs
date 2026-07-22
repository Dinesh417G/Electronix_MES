//! QMS DTOs (§10 `/v1/qms`).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlanInput {
    pub part_id: String,
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Plan {
    pub id: String,
    pub part_id: String,
    pub code: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CharacteristicInput {
    pub plan_id: String,
    pub name: String,
    pub uom: Option<String>,
    #[schema(value_type = Option<String>)]
    pub nominal: Option<Decimal>,
    #[schema(value_type = Option<String>)]
    pub lower_limit: Option<Decimal>,
    #[schema(value_type = Option<String>)]
    pub upper_limit: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Characteristic {
    pub id: String,
    pub plan_id: String,
    pub name: String,
    pub uom: Option<String>,
    #[schema(value_type = Option<String>)]
    pub lower_limit: Option<Decimal>,
    #[schema(value_type = Option<String>)]
    pub upper_limit: Option<Decimal>,
}

/// Record a measured characteristic (auto pass/fail server-side, §8).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResultInput {
    pub characteristic_id: String,
    pub lot_id: Option<String>,
    pub serial_id: Option<String>,
    pub wo_operation_id: Option<String>,
    #[schema(value_type = String)]
    pub measured_value: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InspectionResult {
    pub id: String,
    pub characteristic_id: String,
    pub lot_id: Option<String>,
    pub serial_id: Option<String>,
    #[schema(value_type = String)]
    pub measured_value: Decimal,
    pub result: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Ncr {
    pub id: String,
    pub ncr_no: String,
    pub inspection_result_id: Option<String>,
    pub lot_id: Option<String>,
    pub serial_id: Option<String>,
    pub part_id: Option<String>,
    pub status: String,
    pub disposition: Option<String>,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response to recording a result: the result plus any auto-raised NCR (§8 —
/// a fail auto-creates an NCR and places a hold).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecordResultResponse {
    pub result: InspectionResult,
    pub ncr: Option<Ncr>,
}

/// Apply a disposition to an NCR.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DispositionInput {
    /// One of: rework | scrap | use_as_is | return.
    pub disposition: String,
    pub reason: Option<String>,
}
