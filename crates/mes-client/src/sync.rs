//! Sync + multi-tenancy DTOs (§8.3, §10 `/v1/sync`, §12 M12).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

// ---- Orgs / plants / enrollment ------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrgInput {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Org {
    pub id: String,
    pub code: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlantInput {
    pub code: String,
    pub name: String,
}

/// Returned once at enrollment — carries the plaintext token (never stored).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlantEnrollment {
    pub id: String,
    pub org_id: String,
    pub code: String,
    pub name: String,
    /// Plaintext enrollment token — shown once; the cloud stores only its hash.
    pub token: String,
}

/// A plant as shown on the multi-plant dashboard (no secrets).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlantSummary {
    pub id: String,
    pub org_id: String,
    pub code: String,
    pub name: String,
    pub enrolled: bool,
    pub last_sync_at: Option<DateTime<Utc>>,
}

// ---- Sync protocol -------------------------------------------------------

/// One change-feed entry, carried verbatim end to end. `id` is the idempotency
/// key: applying the same id twice is a no-op (§8.3).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SyncEntry {
    pub id: String,
    pub aggregate: String,
    pub entity_id: String,
    pub op: String,
    #[schema(value_type = Object)]
    pub payload: Value,
}

/// Edge → cloud push of a batch (≤500).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PushRequest {
    pub plant_id: String,
    pub entries: Vec<SyncEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PushResponse {
    /// Entries newly applied on this call.
    pub applied: usize,
    /// Entries skipped because their id was already applied (idempotent replay).
    pub skipped: usize,
}

/// Cloud → edge pull of pending commands for a plant.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PullResponse {
    pub entries: Vec<SyncEntry>,
}

/// Edge acks the command entries it has durably applied so the cloud stops
/// re-sending them.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AckRequest {
    pub plant_id: String,
    pub ids: Vec<String>,
}

/// Create a work order remotely, on the cloud, destined for a plant (§12 M12).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemoteWorkOrderInput {
    pub wo_number: String,
    pub part_id: String,
    #[schema(value_type = String)]
    pub qty_ordered: Decimal,
    pub priority: Option<i32>,
}
