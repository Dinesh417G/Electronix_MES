//! Auth request/response DTOs (§10 `/v1/auth`).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Console/password login request.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Kiosk login request — a PIN, or a scanned badge id. Exactly one is used;
/// PIN takes precedence when both are present.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PinLoginRequest {
    pub username: Option<String>,
    pub pin: Option<String>,
    pub badge_id: Option<String>,
}

/// Successful login response carrying a bearer token.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: String,
    pub username: String,
    pub role_code: String,
    /// Unix epoch seconds at which the token expires.
    pub expires_at: i64,
}
