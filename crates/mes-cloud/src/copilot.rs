//! `/v1/copilot` — the supervisor copilot (§8.6, §12 M13).
//!
//! A stateless tool-use loop: the LLM is offered the read-only `mes-agent-tools`
//! catalog, and any tool it calls is executed **tenant-scoped** through
//! [`mes_agent_tools::dispatch`] — never a raw query here (§14). The model
//! backend is pluggable ([`LlmBackend`]) so the loop is testable with a scripted
//! backend and the real Anthropic key never ships in the desktop binary (§8.6).

use async_trait::async_trait;
use mes_agent_tools::{dispatch, TenantScope, ToolDef};
use mes_client::copilot::{CopilotResponse, CopilotToolCall};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;

#[derive(Debug, thiserror::Error)]
pub enum CopilotError {
    #[error("copilot backend unavailable: {0}")]
    Unavailable(String),
    #[error("llm error: {0}")]
    Llm(String),
    #[error("tool loop did not converge")]
    NoConverge,
}

/// One content block in the Anthropic Messages shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Vec<Block>,
}

/// One assistant turn: its content blocks (text and/or tool_use).
#[derive(Debug, Clone)]
pub struct AssistantReply {
    pub content: Vec<Block>,
}

/// A model backend. The real one calls Anthropic; tests script one.
#[async_trait]
pub trait LlmBackend: Send + Sync {
    async fn turn(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<AssistantReply, CopilotError>;
}

const SYSTEM: &str = "You are the ElectronIx MES supervisor copilot. Answer questions about \
    this plant's production using the read-only tools provided. All tool results are already \
    scoped to the caller's organisation. Be concise and specific.";

const MAX_TURNS: usize = 8;

/// Run the tool-use loop for one user message and return the final answer plus
/// the (tenant-scoped) tools that were called.
pub async fn run_copilot(
    pool: &PgPool,
    backend: &dyn LlmBackend,
    scope: &TenantScope,
    user_message: &str,
) -> Result<CopilotResponse, CopilotError> {
    let tools = mes_agent_tools::catalog();
    let mut messages = vec![Message {
        role: "user".to_string(),
        content: vec![Block::Text {
            text: user_message.to_string(),
        }],
    }];
    let mut tool_calls: Vec<CopilotToolCall> = Vec::new();

    for _ in 0..MAX_TURNS {
        let reply = backend.turn(SYSTEM, &messages, &tools).await?;
        messages.push(Message {
            role: "assistant".to_string(),
            content: reply.content.clone(),
        });

        let uses: Vec<(String, String, Value)> = reply
            .content
            .iter()
            .filter_map(|b| match b {
                Block::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();

        if uses.is_empty() {
            let reply_text = reply
                .content
                .iter()
                .filter_map(|b| match b {
                    Block::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(CopilotResponse {
                reply: reply_text,
                tool_calls,
            });
        }

        // Execute each requested tool tenant-scoped, feed results back.
        let mut results = Vec::new();
        for (id, name, input) in uses {
            tool_calls.push(CopilotToolCall {
                name: name.clone(),
                arguments: input.clone(),
            });
            let content = match dispatch(pool, scope, &name, &input).await {
                Ok(v) => v.to_string(),
                Err(e) => json!({ "error": e.to_string() }).to_string(),
            };
            results.push(Block::ToolResult {
                tool_use_id: id,
                content,
            });
        }
        messages.push(Message {
            role: "user".to_string(),
            content: results,
        });
    }

    Err(CopilotError::NoConverge)
}

// ---- Backends ------------------------------------------------------------

/// Used when no Anthropic key is configured — the copilot degrades gracefully
/// (the desktop panel shows its offline banner, §11).
pub struct NullBackend;

#[async_trait]
impl LlmBackend for NullBackend {
    async fn turn(
        &self,
        _system: &str,
        _messages: &[Message],
        _tools: &[ToolDef],
    ) -> Result<AssistantReply, CopilotError> {
        Err(CopilotError::Unavailable(
            "no model backend configured".to_string(),
        ))
    }
}

/// Real backend: Anthropic Messages API with tool-use. The key stays server-side
/// (§8.6) — it is read from the environment, never shipped to the desktop.
pub struct AnthropicBackend {
    http: reqwest::Client,
    api_key: String,
    model: String,
}

impl AnthropicBackend {
    /// Build from `ANTHROPIC_API_KEY` (+ optional `MES_COPILOT_MODEL`). Returns
    /// `None` when no key is set, so the caller falls back to [`NullBackend`].
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())?;
        let model =
            std::env::var("MES_COPILOT_MODEL").unwrap_or_else(|_| "claude-sonnet-5".to_string());
        Some(Self {
            http: reqwest::Client::builder().build().unwrap_or_default(),
            api_key,
            model,
        })
    }
}

#[async_trait]
impl LlmBackend for AnthropicBackend {
    async fn turn(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<AssistantReply, CopilotError> {
        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "system": system,
            "tools": tools.iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })).collect::<Vec<_>>(),
            "messages": messages,
        });

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| CopilotError::Llm(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(CopilotError::Llm(format!(
                "anthropic returned {}",
                resp.status()
            )));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| CopilotError::Llm(e.to_string()))?;
        let content: Vec<Block> =
            serde_json::from_value(v.get("content").cloned().unwrap_or(json!([])))
                .map_err(|e| CopilotError::Llm(format!("bad content: {e}")))?;
        Ok(AssistantReply { content })
    }
}
