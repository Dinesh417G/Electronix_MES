//! `mes-agent-tools` — shared read-only query tools for MCP *and* copilot (§8.6,
//! §12 M13).
//!
//! One tool implementation, two front doors: the MCP server and the
//! `/v1/copilot` endpoint both call [`dispatch`] and nothing else touches the DB
//! on their behalf. Tenant scoping is enforced **here**, at the query layer
//! (§14), so a bug in either transport can never leak across tenants: every
//! query is bound to a [`TenantScope`] (an org id) and joins through
//! `plants.plant_id`. All tools are read-only in v1 (§8.6, §16).

#![forbid(unsafe_code)]

pub mod tools;

use serde_json::Value;
use sqlx::PgPool;

#[derive(Debug, thiserror::Error)]
pub enum AgentToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("query error: {0}")]
    Query(String),
}

/// The tenant the tools are allowed to see. Constructed only from an
/// authenticated org token by the front doors — the tools never widen it.
#[derive(Debug, Clone)]
pub struct TenantScope {
    pub org_id: String,
}

impl TenantScope {
    pub fn new(org_id: impl Into<String>) -> Self {
        Self {
            org_id: org_id.into(),
        }
    }
}

/// The single entry point both front doors use. Dispatches to a read-only tool,
/// always bound to `scope`.
pub async fn dispatch(
    pool: &PgPool,
    scope: &TenantScope,
    name: &str,
    args: &Value,
) -> Result<Value, AgentToolError> {
    match name {
        "get_wo_status" => tools::get_wo_status(pool, scope, args).await,
        "get_oee" => tools::get_oee(pool, scope, args).await,
        "get_downtime_pareto" => tools::get_downtime_pareto(pool, scope, args).await,
        "get_ncr_queue" => tools::get_ncr_queue(pool, scope, args).await,
        "get_trace" => tools::get_trace(pool, scope, args).await,
        "get_maintenance_due" => tools::get_maintenance_due(pool, scope, args).await,
        other => Err(AgentToolError::UnknownTool(other.to_string())),
    }
}

/// The tool catalog exposed to both front doors — name, description, and a JSON
/// Schema for the arguments (MCP `tools/list` and the copilot's `tools` array).
pub fn catalog() -> Vec<ToolDef> {
    tools::catalog()
}

/// A tool's public definition (MCP/Anthropic-compatible shape).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema for the tool's input object.
    pub input_schema: Value,
}
