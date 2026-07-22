//! Axum extractors for authentication and authorization.
//!
//! `AuthUser` validates the bearer token and yields the caller's identity and
//! role. `MasterWriter` layers the master-data write policy (§12 M1) on top, so
//! a handler that takes `MasterWriter` simply cannot run for a role that may not
//! write — the gate is structural, not a forgotten `if` inside the body.

use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::Json;
use mes_client::ApiError;

use crate::auth::decode_token;
use crate::http::AppState;

/// Rejection returned when auth/authorization fails.
pub struct AuthRejection(StatusCode, &'static str, String);

impl axum::response::IntoResponse for AuthRejection {
    fn into_response(self) -> axum::response::Response {
        (self.0, Json(ApiError::new(self.1, self.2))).into_response()
    }
}

/// An authenticated caller.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub role: String,
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AuthRejection(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "missing Authorization header".to_string(),
                )
            })?;

        let token = header.strip_prefix("Bearer ").ok_or_else(|| {
            AuthRejection(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "expected Bearer token".to_string(),
            )
        })?;

        let claims = decode_token(&state.auth, token).map_err(|_| {
            AuthRejection(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "invalid or expired token".to_string(),
            )
        })?;

        Ok(AuthUser {
            user_id: claims.sub,
            role: claims.role,
        })
    }
}

/// An authenticated caller who is permitted to write master data (Admin or
/// Planner, §12 M1). Rejects everyone else with 403.
pub struct MasterWriter(pub AuthUser);

#[async_trait]
impl FromRequestParts<AppState> for MasterWriter {
    type Rejection = AuthRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if mes_core::roles::can_write_master(&user.role) {
            Ok(MasterWriter(user))
        } else {
            Err(AuthRejection(
                StatusCode::FORBIDDEN,
                "forbidden",
                "role may not modify master data".to_string(),
            ))
        }
    }
}
