//! HTTP surface for `mes-cloud`.
//!
//! M0 exposes liveness (`/healthz`) and readiness (`/readyz`) only. Sync,
//! copilot, and the MCP transport mount here from M12/M13 (§10).

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use mes_client::HealthResponse;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;

/// Name reported in health payloads and OpenAPI metadata.
const SERVICE: &str = "mes-cloud";

/// Shared application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    /// `None` until a database is configured (M0 allows liveness-only boot).
    pub pool: Option<PgPool>,
    /// Optional bearer gating org/plant provisioning (§12 M12). `None` = open.
    pub admin_token: Option<String>,
}

/// OpenAPI document root for the cloud service (§10).
#[derive(OpenApi)]
#[openapi(
    paths(healthz),
    components(schemas(HealthResponse)),
    info(title = "ElectronIx MES — Cloud API", version = "0.1.0")
)]
pub struct ApiDoc;

/// Build the cloud router with health endpoints and the trace layer.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api-doc/openapi.json", get(openapi_json))
        .nest("/v1/sync", crate::sync::routes())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Serve the OpenAPI document as JSON (§10 — utoipa-documented API).
async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// Liveness probe — succeeds whenever the process can serve requests.
#[utoipa::path(
    get,
    path = "/healthz",
    responses((status = 200, description = "Service is alive", body = HealthResponse))
)]
async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse::ok(SERVICE, mes_core::VERSION))
}

/// Readiness probe — reports whether the database is usable.
async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    if is_ready(&state).await {
        (
            StatusCode::OK,
            Json(HealthResponse::ok(SERVICE, mes_core::VERSION)),
        )
            .into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                service: SERVICE.to_string(),
                status: "not_ready".to_string(),
                version: mes_core::VERSION.to_string(),
            }),
        )
            .into_response()
    }
}

/// True when the database is reachable.
async fn is_ready(state: &AppState) -> bool {
    match &state.pool {
        Some(pool) => sqlx::query("SELECT 1").execute(pool).await.is_ok(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_doc_builds() {
        let doc = ApiDoc::openapi();
        assert_eq!(doc.info.title, "ElectronIx MES — Cloud API");
    }

    #[tokio::test]
    async fn healthz_reports_ok() {
        let Json(body) = healthz().await;
        assert_eq!(body.status, "ok");
        assert_eq!(body.service, "mes-cloud");
    }

    #[tokio::test]
    async fn readiness_is_false_without_pool() {
        let state = AppState {
            pool: None,
            admin_token: None,
        };
        assert!(!is_ready(&state).await);
    }
}
