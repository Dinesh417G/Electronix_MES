//! Cloud integration-test harness (§13 — fresh schema per test). Mirrors the
//! edge harness: each test gets its own migrated Postgres schema (isolated via
//! `search_path`), a built router, and helpers to drive it in-process. Gated on
//! `DATABASE_URL` so suites skip cleanly where no database is available.

#![allow(dead_code)]

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use mes_cloud::http::{router, AppState};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use tower::ServiceExt;

pub struct Ctx {
    pub state: AppState,
    pub admin_pool: PgPool,
    pub schema: String,
}

impl Ctx {
    pub fn router(&self) -> axum::Router {
        router(self.state.clone())
    }
    pub fn pool(&self) -> &PgPool {
        self.state.pool.as_ref().unwrap()
    }
}

static SETUP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Build an isolated schema + migrate it. Returns `None` when no database is
/// configured so a suite is a no-op locally instead of a hard failure.
pub async fn setup() -> Option<Ctx> {
    let url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.is_empty())?;

    let _guard = SETUP_LOCK.lock().await;

    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("connect admin pool");

    let schema = format!("t_{}", mes_core::new_id().to_lowercase());
    admin_pool
        .execute(format!("CREATE SCHEMA \"{schema}\"").as_str())
        .await
        .expect("create schema");

    let schema_for_hook = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .after_connect(move |conn, _meta| {
            let schema = schema_for_hook.clone();
            Box::pin(async move {
                conn.execute(format!("SET search_path TO \"{schema}\", public").as_str())
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .expect("connect app pool");

    mes_db::run_migrations(&pool).await.expect("migrate");

    let state = AppState {
        pool: Some(pool),
        admin_token: None,
    };
    Some(Ctx {
        state,
        admin_pool,
        schema,
    })
}

pub async fn teardown(ctx: Ctx) {
    let _ = ctx
        .admin_pool
        .execute(format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", ctx.schema).as_str())
        .await;
}

/// Issue a request and return (status, json body). `token` is sent as a bearer.
pub async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let req = if let Some(b) = body {
        req.header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string()))
            .unwrap()
    } else {
        req.body(Body::empty()).unwrap()
    };

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json)
}

/// Seed a part directly (master data is provisioned before WOs sync).
pub async fn seed_part(pool: &PgPool, id: &str, code: &str) {
    sqlx::query("INSERT INTO parts (id, code, name, uom) VALUES ($1, $2, $2, 'ea')")
        .bind(id)
        .bind(code)
        .execute(pool)
        .await
        .expect("seed part");
}
