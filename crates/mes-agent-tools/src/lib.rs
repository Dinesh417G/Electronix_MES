//! `mes-agent-tools` — shared read-only query tools for MCP *and* copilot (§8.6, M13).
//!
//! One tool implementation, two front doors: the `rmcp` MCP server and the
//! `/v1/copilot` endpoint both call these functions and nowhere else. Tenant
//! scoping is enforced here at the query layer (§14) so a bug in either front
//! door's transport can never leak across tenants. All tools are read-only in
//! v1 (§8.6, §16). M0 lands only the crate shell.

#![forbid(unsafe_code)]

#[derive(Debug, thiserror::Error)]
pub enum AgentToolError {
    #[error("query error: {0}")]
    Query(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_renders() {
        let e = AgentToolError::Query("boom".into());
        assert_eq!(e.to_string(), "query error: boom");
    }
}
