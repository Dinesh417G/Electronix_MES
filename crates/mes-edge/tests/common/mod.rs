//! Shared integration-test harness (§13 — fresh schema per test).
//!
//! Each test gets its own migrated Postgres schema, isolated via `search_path`,
//! plus helpers to drive the router in-process and mint tokens. Gated on
//! `DATABASE_URL`: `setup()` returns `None` where no database is available so
//! suites skip cleanly instead of failing.

#![allow(dead_code)] // Not every test uses every helper.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use mes_edge::auth::AuthConfig;
use mes_edge::http::{router, AppState};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use tower::ServiceExt; // for `oneshot`

/// Per-test context: an isolated schema + a built router.
pub struct Ctx {
    pub state: AppState,
    pub admin_pool: PgPool,
    pub schema: String,
}

impl Ctx {
    pub fn router(&self) -> axum::Router {
        router(self.state.clone())
    }
}

/// Serializes the schema-create + migrate critical section across parallel test
/// threads. Migration 0001 runs `CREATE EXTENSION IF NOT EXISTS timescaledb`,
/// which can race if several tests apply it at once; holding this lock during
/// setup makes the first application win and the rest no-op.
static SETUP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Build an isolated schema and migrate it. Returns `None` when no database is
/// configured so a suite is a no-op locally instead of a hard failure.
pub async fn setup() -> Option<Ctx> {
    let url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.is_empty())?;

    let _setup_guard = SETUP_LOCK.lock().await;

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
                // Test schema first (so unqualified CREATE TABLE lands there and
                // stays isolated), then public so the timescaledb extension and
                // its catalog functions still resolve.
                conn.execute(format!("SET search_path TO \"{schema}\", public").as_str())
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .expect("connect app pool");

    mes_db::run_migrations(&pool).await.expect("migrate");

    let state = AppState::new(
        Some(pool),
        AuthConfig::new("integration-test-secret".to_string(), 3600),
    );
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

/// Issue a request and return (status, json body).
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

/// Seed a user directly via the repo and mint a token for them.
pub async fn seed_user_token(ctx: &Ctx, username: &str, role: &str) -> String {
    let pool = ctx.state.pool.as_ref().unwrap();
    let pw = mes_edge::auth::hash_secret("pw").unwrap();
    let user = mes_db::repo::create_user(pool, username, username, role, Some(&pw), None, None)
        .await
        .expect("seed user");
    let (token, _) = mes_edge::auth::issue_token(&ctx.state.auth, &user.id, role).unwrap();
    token
}
