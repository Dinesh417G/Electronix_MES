//! Execution DTOs (§10 `/v1/exec`) — operator actions on the shop floor.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Record good/scrap counts against an operation. A `scrap_reason_id` is
/// required whenever `scrap > 0` (§11 — scrap forces a reason pick).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CountInput {
    #[serde(default)]
    pub good: i32,
    #[serde(default)]
    pub scrap: i32,
    pub scrap_reason_id: Option<String>,
    /// When the count occurred; defaults to now if omitted.
    pub ts: Option<DateTime<Utc>>,
}

/// Classify a downtime event with a reason (§8.1 — reasons attached by operator).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ClassifyDowntimeInput {
    pub reason_id: String,
}

/// Split a downtime event at `at` into two events, optionally classifying each.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SplitDowntimeInput {
    pub at: DateTime<Utc>,
    pub first_reason_id: Option<String>,
    pub second_reason_id: Option<String>,
}

/// A downtime event as returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DowntimeEventDto {
    pub id: String,
    pub work_center_id: String,
    pub state: String,
    pub start_ts: DateTime<Utc>,
    pub end_ts: DateTime<Utc>,
    pub reason_id: Option<String>,
}
