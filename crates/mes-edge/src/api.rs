//! Shared HTTP helpers: error translation and state access.

use axum::http::StatusCode;
use axum::Json;
use mes_client::ApiError;
use mes_db::repo::RepoError;
use sqlx::PgPool;

use crate::http::AppState;

/// Uniform error type returned by handlers; the tuple implements `IntoResponse`.
pub type ApiErr = (StatusCode, Json<ApiError>);

/// Build an error response.
pub fn err(status: StatusCode, code: &str, msg: impl Into<String>) -> ApiErr {
    (status, Json(ApiError::new(code, msg)))
}

/// Translate a repository error into the appropriate HTTP status.
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

/// Access the pool, or 503 when the service booted without a database.
pub fn require_pool(state: &AppState) -> Result<&PgPool, ApiErr> {
    state.pool.as_ref().ok_or_else(|| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "database not configured",
        )
    })
}

/// Record an audit entry, best-effort: a failure is logged but never fails the
/// caller's request (the mutation it describes has already succeeded).
pub async fn audit(
    pool: &PgPool,
    actor_id: Option<&str>,
    action: &str,
    entity: &str,
    entity_id: Option<&str>,
    detail: Option<serde_json::Value>,
) {
    if let Err(e) =
        mes_db::repo::insert_audit(pool, actor_id, action, entity, entity_id, detail).await
    {
        tracing::error!(error = %e, action, entity, "failed to write audit log");
    }
}
