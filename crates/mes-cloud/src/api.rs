//! Shared HTTP helpers for `mes-cloud` — error translation + state access.

use axum::http::StatusCode;
use axum::Json;
use mes_client::ApiError;
use mes_db::repo::RepoError;
use sqlx::PgPool;

use crate::http::AppState;

pub type ApiErr = (StatusCode, Json<ApiError>);

pub fn err(status: StatusCode, code: &str, msg: impl Into<String>) -> ApiErr {
    (status, Json(ApiError::new(code, msg)))
}

pub fn repo_err(e: RepoError) -> ApiErr {
    match e {
        RepoError::NotFound => err(StatusCode::NOT_FOUND, "not_found", "resource not found"),
        RepoError::Conflict(m) => err(StatusCode::CONFLICT, "conflict", m),
        RepoError::InvalidReference(m) => err(StatusCode::BAD_REQUEST, "invalid_reference", m),
        RepoError::Db(e) => {
            tracing::error!(error = %e, "database error");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "database error",
            )
        }
    }
}

pub fn require_pool(state: &AppState) -> Result<&PgPool, ApiErr> {
    state.pool.as_ref().ok_or_else(|| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "database not configured",
        )
    })
}
