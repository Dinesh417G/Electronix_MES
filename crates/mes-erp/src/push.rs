//! The generic outbound REST client used by "sync now" (§8, §10, M10).
//!
//! One POST of a JSON payload to the connection's configured `endpoint_url`,
//! bearer-authenticated with the (decrypted) token. Deliberately shape-agnostic:
//! the payload has already been mapped to the ERP's vocabulary by
//! [`crate::FieldMapping`], so this layer knows nothing customer-specific.

use serde_json::Value;

use crate::ErpError;

/// A thin, reusable REST client. Proxy is disabled so plant-local / test ERP
/// endpoints on loopback are reached directly.
#[derive(Debug, Clone)]
pub struct ErpClient {
    http: reqwest::Client,
}

impl Default for ErpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ErpClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap_or_default();
        Self { http }
    }

    /// POST `body` to `url`, bearer-authing with `token` when present. Returns
    /// the parsed JSON response body (or `Null` when the ERP returns no JSON).
    pub async fn post_json(
        &self,
        url: &str,
        token: Option<&str>,
        body: &Value,
    ) -> Result<Value, ErpError> {
        let mut req = self.http.post(url).json(body);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ErpError::Http(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(ErpError::Http(format!("erp returned {status}")));
        }
        Ok(resp.json::<Value>().await.unwrap_or(Value::Null))
    }
}
