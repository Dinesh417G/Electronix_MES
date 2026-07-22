//! HTTP surface for `mes-edge`.
//!
//! M0 exposes liveness (`/healthz`) and readiness (`/readyz`) only. Feature
//! routers (`/v1/*`, `/ws`) are mounted here from M1 onward (§10). Every
//! handler runs inside a tracing span via `TraceLayer` (§14).

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use mes_client::ws::WsEvent;
use mes_client::HealthResponse;
use mes_dnc_bridge::{DisconnectedDaemon, DncDaemon};
use sqlx::PgPool;
use tokio::sync::broadcast;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;

use crate::auth::AuthConfig;

/// Name reported in health payloads and OpenAPI metadata.
const SERVICE: &str = "mes-edge";

/// Shared application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    /// `None` until a database is configured (M0 allows liveness-only boot).
    pub pool: Option<PgPool>,
    /// JWT signing/verification config for authenticated routes (§12 M1).
    pub auth: AuthConfig,
    /// Broadcast bus for live `/ws` events (§10, §12 M3).
    pub events: broadcast::Sender<WsEvent>,
    /// Command channel to the `dnc-daemon` (§8.4, §12 M4). Defaults to a
    /// disconnected stub so orchestration degrades gracefully when no CNC is
    /// present; tests inject a virtual daemon.
    pub dnc: Arc<dyn DncDaemon>,
}

impl AppState {
    /// Build state with a fresh event bus and a disconnected DNC daemon.
    pub fn new(pool: Option<PgPool>, auth: AuthConfig) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            pool,
            auth,
            events,
            dnc: Arc::new(DisconnectedDaemon),
        }
    }

    /// Replace the DNC daemon handle (production wiring / tests).
    pub fn with_dnc(mut self, dnc: Arc<dyn DncDaemon>) -> Self {
        self.dnc = dnc;
        self
    }

    /// Publish a live event; a send with no subscribers is not an error.
    pub fn publish(&self, event: WsEvent) {
        let _ = self.events.send(event);
    }
}

/// OpenAPI document root. Grows as `/v1/*` handlers are annotated (§10).
#[derive(OpenApi)]
#[openapi(
    paths(healthz),
    components(schemas(HealthResponse)),
    info(title = "ElectronIx MES — Edge API", version = "0.1.0")
)]
pub struct ApiDoc;

/// Build the edge router with health endpoints and the trace layer.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api-doc/openapi.json", get(openapi_json))
        .route("/ws", get(crate::ws::ws_handler))
        .nest("/v1/auth", crate::auth_routes::routes())
        .nest("/v1/master", crate::master::routes())
        .nest("/v1/ingest", crate::ingest::routes())
        .nest("/v1/orders", crate::orders::routes())
        .nest("/v1/exec", crate::exec::routes())
        .nest("/v1/dnc", crate::dnc::routes())
        .nest("/v1/analytics", crate::analytics::routes())
        .nest("/v1/trace", crate::trace::routes())
        .nest("/v1/qms", crate::qms::routes())
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

/// Readiness probe — reports whether dependencies (the database) are usable.
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

/// True when every wired dependency is reachable. With no database configured
/// the edge is considered not-ready (it cannot persist), which is the correct
/// signal for an orchestrator even though liveness still passes.
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
        assert_eq!(doc.info.title, "ElectronIx MES — Edge API");
    }

    #[tokio::test]
    async fn healthz_reports_ok() {
        let Json(body) = healthz().await;
        assert_eq!(body.status, "ok");
        assert_eq!(body.service, "mes-edge");
    }

    #[tokio::test]
    async fn readiness_is_false_without_pool() {
        let state = AppState::new(None, AuthConfig::new("test".to_string(), 3600));
        assert!(!is_ready(&state).await);
    }
}
