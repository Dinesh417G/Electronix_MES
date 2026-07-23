//! `github` — ship a redacted diagnostic bundle to a private GitHub repo, as an
//! issue (mirrors DNC's shipping). Shipping is **opt-in per customer** (§8.5,
//! §17 Q4): with `enabled = false` nothing ever leaves the box.

use async_trait::async_trait;

use crate::DiagnosticsError;

/// Per-customer shipping configuration. Secrets come from the environment (§14).
#[derive(Debug, Clone)]
pub struct ShipConfig {
    /// Opt-in switch — off by default; a customer turns it on in settings.
    pub enabled: bool,
    /// Target repo, `owner/name`.
    pub repo: String,
    /// GitHub token with issues:write on the private repo.
    pub token: String,
}

impl ShipConfig {
    /// Read from env: `MES_DIAG_ENABLED`, `MES_DIAG_REPO`, `MES_DIAG_TOKEN`.
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("MES_DIAG_ENABLED").as_deref() == Ok("true"),
            repo: std::env::var("MES_DIAG_REPO").unwrap_or_default(),
            token: std::env::var("MES_DIAG_TOKEN").unwrap_or_default(),
        }
    }
}

/// A destination for a diagnostic bundle. Trait so the send path is testable
/// without touching the network.
#[async_trait]
pub trait Shipper: Send + Sync {
    async fn ship(&self, title: &str, body: &str) -> Result<(), DiagnosticsError>;
}

/// Ships bundles as issues on a private GitHub repo.
pub struct GitHubShipper {
    http: reqwest::Client,
    repo: String,
    token: String,
}

impl GitHubShipper {
    pub fn new(repo: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::builder().build().unwrap_or_default(),
            repo: repo.into(),
            token: token.into(),
        }
    }
}

#[async_trait]
impl Shipper for GitHubShipper {
    async fn ship(&self, title: &str, body: &str) -> Result<(), DiagnosticsError> {
        let url = format!("https://api.github.com/repos/{}/issues", self.repo);
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "electronix-mes-diagnostics")
            .json(&serde_json::json!({
                "title": title,
                "body": body,
                "labels": ["diagnostics"],
            }))
            .send()
            .await
            .map_err(|e| DiagnosticsError::Ship(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(DiagnosticsError::Ship(format!(
                "github returned {}",
                resp.status()
            )));
        }
        Ok(())
    }
}
