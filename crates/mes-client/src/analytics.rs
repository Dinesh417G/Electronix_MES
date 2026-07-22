//! Analytics DTOs (§10 `/v1/analytics`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Time-window query parameters shared by the analytics endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// One point on a daily downtime trend.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrendPoint {
    pub day: DateTime<Utc>,
    pub seconds: i64,
}

/// OEE for one shift occurrence.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ShiftOee {
    pub shift_name: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub availability: f64,
    pub performance: f64,
    pub quality: f64,
    pub oee: f64,
}
