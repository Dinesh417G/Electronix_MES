//! DNC orchestration DTOs (§10 `/v1/dnc`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A DNC transfer as MES tracks it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransferEvent {
    pub id: String,
    pub wo_operation_id: Option<String>,
    pub program_id: String,
    pub direction: String,
    pub status: String,
    pub dnc_daemon_ref: Option<String>,
    pub triggered_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Manually trigger (or retry) a transfer for a program.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ManualTransferInput {
    pub program_id: String,
    pub wo_operation_id: Option<String>,
    pub machine: Option<String>,
}

/// An operator-edited program revision awaiting supervisor review.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProgramRevision {
    pub id: String,
    pub program_id: String,
    pub revision_no: i32,
    pub source: String,
    pub content_ref: Option<String>,
    pub status: String,
    pub submitted_by: Option<String>,
    pub submitted_at: DateTime<Utc>,
    pub promoted_by: Option<String>,
    pub promoted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
