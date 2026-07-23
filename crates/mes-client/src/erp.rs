//! ERP integration DTOs (§10 `/v1/erp`) — connection config, generic
//! import/export, and the sync log (§7, §12 M10).
//!
//! Security: the auth token is **write-only**. It is accepted on create/update,
//! encrypted at rest, and never returned — responses expose only `has_token`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Create/replace an ERP connection (the admin integration page's form).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErpConnectionInput {
    pub site_id: Option<String>,
    pub name: String,
    pub endpoint_url: String,
    /// Plaintext token; encrypted at rest and never returned. Omit to leave an
    /// existing token unchanged on update.
    pub auth_token: Option<String>,
    /// `{ "fields": { "<canonical>": "<external>" } }`.
    #[schema(value_type = Object)]
    #[serde(default)]
    pub field_mapping: Value,
    /// `import` | `export` | `both`.
    pub direction: Option<String>,
    pub enabled: Option<bool>,
}

/// An ERP connection as returned to clients — **never** carries the token.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErpConnection {
    pub id: String,
    pub site_id: Option<String>,
    pub name: String,
    pub endpoint_url: String,
    /// Whether a token is stored (the value itself is never exposed).
    pub has_token: bool,
    #[schema(value_type = Object)]
    pub field_mapping: Value,
    pub direction: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One row of the sync audit trail.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErpSyncLogEntry {
    pub id: String,
    pub connection_id: Option<String>,
    pub direction: String,
    pub entity: String,
    pub record_count: i32,
    pub status: String,
    pub detail: Option<String>,
    pub ts: DateTime<Utc>,
}

/// Import external records into MES through a connection's mapping.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErpImportRequest {
    pub connection_id: String,
    /// Supported: `work_order`.
    pub entity: String,
    /// Records in the ERP's own shape; mapped to canonical before creation.
    #[schema(value_type = Vec<Object>)]
    pub records: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErpImportResult {
    pub entity: String,
    pub imported: usize,
    pub ids: Vec<String>,
    pub sync_log_id: String,
}

/// Export MES data out to the ERP endpoint ("sync now") through the mapping.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErpExportRequest {
    pub connection_id: String,
    /// Supported: `stock_level`, `procurement_request`.
    pub entity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErpExportResult {
    pub entity: String,
    pub record_count: usize,
    /// Whether the outbound POST to the ERP endpoint succeeded.
    pub pushed: bool,
    /// The payload as sent to the ERP (already in the ERP's field vocabulary).
    #[schema(value_type = Vec<Object>)]
    pub payload: Vec<Value>,
    pub sync_log_id: String,
}
