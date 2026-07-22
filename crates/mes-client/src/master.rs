//! Master-data DTOs (§10 `/v1/master`).
//!
//! Create/update requests and response representations for the equipment
//! hierarchy (site → area → work_center) and products (part), plus user
//! creation. Timestamps are UTC (§14); the site timezone drives shift/OEE math.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ---- Site ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SiteInput {
    pub code: String,
    pub name: String,
    /// IANA timezone; defaults to `Asia/Kolkata` when omitted.
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Site {
    pub id: String,
    pub code: String,
    pub name: String,
    pub timezone: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---- Area ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AreaInput {
    pub site_id: String,
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Area {
    pub id: String,
    pub site_id: String,
    pub code: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---- Work center ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkCenterInput {
    pub area_id: String,
    pub code: String,
    pub name: String,
    pub external_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkCenter {
    pub id: String,
    pub area_id: String,
    pub code: String,
    pub name: String,
    pub external_ref: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---- Part ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PartInput {
    pub code: String,
    pub name: String,
    /// Unit of measure; defaults to `ea` when omitted.
    pub uom: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Part {
    pub id: String,
    pub code: String,
    pub name: String,
    pub uom: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---- User ----------------------------------------------------------------

/// Create-user request. Password/PIN/badge are all optional so a kiosk-only
/// operator can be created with just a PIN or badge (§7). Secrets are hashed
/// server-side before storage (§14).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserInput {
    pub username: String,
    pub display_name: String,
    pub role_code: String,
    pub password: Option<String>,
    pub pin: Option<String>,
    pub badge_id: Option<String>,
}

/// User representation — never carries any secret material.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role_code: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---- Program (routing_op ↔ DNC program library, §7) ----------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProgramInput {
    pub routing_op_id: Option<String>,
    pub part_id: Option<String>,
    /// The identifier `dnc-daemon` knows the program by (§8.4).
    pub program_identifier: String,
    pub target_machine: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Program {
    pub id: String,
    pub routing_op_id: Option<String>,
    pub part_id: Option<String>,
    pub program_identifier: String,
    pub target_machine: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
