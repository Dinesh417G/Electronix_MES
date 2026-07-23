//! Copilot DTOs (§10 `/v1/copilot`, §12 M13).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// A copilot request from the supervisor panel. Stateless: prior turns are
/// replayed by the client (the copilot stores only an audit log, §7).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CopilotRequest {
    pub message: String,
}

/// One tool call the copilot made while answering (surfaced for transparency).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CopilotToolCall {
    pub name: String,
    #[schema(value_type = Object)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CopilotResponse {
    pub reply: String,
    /// The read-only tools the copilot invoked to answer (all tenant-scoped).
    pub tool_calls: Vec<CopilotToolCall>,
}
