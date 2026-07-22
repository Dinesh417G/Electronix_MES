//! M1 acceptance tests — master-data CRUD + role enforcement (§12 M1, §13).
//!
//! Each test runs against its own freshly-migrated Postgres schema (§13
//! "fresh schema per test"), so tests never interfere. The suite is gated on
//! `DATABASE_URL`: it runs in CI (which provides a TimescaleDB service) and is
//! skipped where no database is available.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use mes_edge::auth::AuthConfig;
use mes_edge::http::{router, AppState};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use tower::ServiceExt; // for `oneshot`

/// Per-test context: an isolated schema + a built router.
struct Ctx {
    state: AppState,
    admin_pool: PgPool,
    schema: String,
}

impl Ctx {
    fn router(&self) -> axum::Router {
        router(self.state.clone())
    }
}

/// Serializes the schema-create + migrate critical section across the parallel
/// test threads. Migration 0001 runs `CREATE EXTENSION IF NOT EXISTS
/// timescaledb`, which can race if several tests apply it at once; holding this
/// lock during setup makes the first application win and the rest no-op.
static SETUP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Build an isolated schema and migrate it. Returns `None` when no database is
/// configured so the suite is a no-op locally instead of a hard failure.
async fn setup() -> Option<Ctx> {
    let url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.is_empty())?;

    let _setup_guard = SETUP_LOCK.lock().await;

    // Admin connection used to create/drop the throwaway schema.
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

    // App pool pinned to the new schema via search_path on every connection.
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

    let state = AppState {
        pool: Some(pool),
        auth: AuthConfig::new("integration-test-secret".to_string(), 3600),
    };
    Some(Ctx {
        state,
        admin_pool,
        schema,
    })
}

async fn teardown(ctx: Ctx) {
    // Drop the isolated schema. Best-effort — the test has already asserted.
    let _ = ctx
        .admin_pool
        .execute(format!("DROP SCHEMA IF EXISTS \"{}\" CASCADE", ctx.schema).as_str())
        .await;
}

/// Issue a request and return (status, json body).
async fn call(
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
async fn seed_user_token(ctx: &Ctx, username: &str, role: &str) -> String {
    let pool = ctx.state.pool.as_ref().unwrap();
    let pw = mes_edge::auth::hash_secret("pw").unwrap();
    let user = mes_db::repo::create_user(pool, username, username, role, Some(&pw), None, None)
        .await
        .expect("seed user");
    let (token, _) = mes_edge::auth::issue_token(&ctx.state.auth, &user.id, role).unwrap();
    token
}

#[tokio::test]
async fn roles_are_seeded() {
    let Some(ctx) = setup().await else {
        return;
    };
    let pool = ctx.state.pool.as_ref().unwrap();
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM roles")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(n, 5, "Admin/Planner/Supervisor/Operator/Quality seeded");
    teardown(ctx).await;
}

#[tokio::test]
async fn admin_can_crud_master_data() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let token = seed_user_token(&ctx, "admin1", mes_core::roles::ADMIN).await;

    // Create a part.
    let (status, body) = call(
        &app,
        "POST",
        "/v1/master/parts",
        Some(&token),
        Some(json!({"code": "P-100", "name": "Widget"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    let part_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["uom"], "ea", "default uom applied");

    // Read it back.
    let (status, body) = call(
        &app,
        "GET",
        &format!("/v1/master/parts/{part_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], "P-100");

    // Update it.
    let (status, body) = call(
        &app,
        "PUT",
        &format!("/v1/master/parts/{part_id}"),
        Some(&token),
        Some(json!({"code": "P-100", "name": "Widget v2", "uom": "kg"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Widget v2");
    assert_eq!(body["uom"], "kg");

    // List shows exactly one.
    let (status, body) = call(&app, "GET", "/v1/master/parts", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);

    // Delete it.
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/v1/master/parts/{part_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Gone.
    let (status, _) = call(
        &app,
        "GET",
        &format!("/v1/master/parts/{part_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Every mutation was audited (create, update, delete).
    let pool = ctx.state.pool.as_ref().unwrap();
    let n = mes_db::repo::count_audit_for_entity(pool, "part")
        .await
        .unwrap();
    assert_eq!(n, 3, "create+update+delete audited");

    teardown(ctx).await;
}

#[tokio::test]
async fn operator_cannot_write_master_but_can_read() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let operator = seed_user_token(&ctx, "op1", mes_core::roles::OPERATOR).await;
    let admin = seed_user_token(&ctx, "admin2", mes_core::roles::ADMIN).await;

    // Operator write is forbidden (§12 M1 acceptance).
    let (status, body) = call(
        &app,
        "POST",
        "/v1/master/parts",
        Some(&operator),
        Some(json!({"code": "P-1", "name": "X"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(body["error"], "forbidden");

    // No part was created despite the attempt.
    let pool = ctx.state.pool.as_ref().unwrap();
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM parts")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(n, 0);

    // Admin creates one; operator CAN read it.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/master/parts",
        Some(&admin),
        Some(json!({"code": "P-1", "name": "X"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = call(&app, "GET", "/v1/master/parts", Some(&operator), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.as_array().unwrap().len(),
        1,
        "operator may read master"
    );

    teardown(ctx).await;
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();

    // No token → 401 on both read and write.
    let (status, _) = call(&app, "GET", "/v1/master/parts", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = call(
        &app,
        "POST",
        "/v1/master/parts",
        None,
        Some(json!({"code": "P", "name": "N"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    teardown(ctx).await;
}

#[tokio::test]
async fn password_login_issues_working_token() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();

    // Seed an admin with a known password via the repo.
    let pool = ctx.state.pool.as_ref().unwrap();
    let pw = mes_edge::auth::hash_secret("s3cret").unwrap();
    mes_db::repo::create_user(
        pool,
        "boss",
        "Boss",
        mes_core::roles::ADMIN,
        Some(&pw),
        None,
        None,
    )
    .await
    .unwrap();

    // Wrong password → 401.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/auth/login",
        None,
        Some(json!({"username": "boss", "password": "nope"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Correct password → token that then authorizes a master write.
    let (status, body) = call(
        &app,
        "POST",
        "/v1/auth/login",
        None,
        Some(json!({"username": "boss", "password": "s3cret"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let token = body["token"].as_str().unwrap();
    assert_eq!(body["role_code"], "Admin");

    let (status, _) = call(
        &app,
        "POST",
        "/v1/master/sites",
        Some(token),
        Some(json!({"code": "S1", "name": "Plant 1"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    teardown(ctx).await;
}

#[tokio::test]
async fn equipment_hierarchy_enforces_references() {
    let Some(ctx) = setup().await else {
        return;
    };
    let app = ctx.router();
    let admin = seed_user_token(&ctx, "admin3", mes_core::roles::ADMIN).await;

    // Work center against a non-existent area → 400 invalid_reference.
    let (status, body) = call(
        &app,
        "POST",
        "/v1/master/work-centers",
        Some(&admin),
        Some(json!({"area_id": "does-not-exist", "code": "WC1", "name": "Lathe"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");

    // Build site → area → work center properly.
    let (_, site) = call(
        &app,
        "POST",
        "/v1/master/sites",
        Some(&admin),
        Some(json!({"code": "S1", "name": "Plant"})),
    )
    .await;
    let (_, area) = call(
        &app,
        "POST",
        "/v1/master/areas",
        Some(&admin),
        Some(json!({"site_id": site["id"], "code": "A1", "name": "Machining"})),
    )
    .await;
    let (status, wc) = call(
        &app,
        "POST",
        "/v1/master/work-centers",
        Some(&admin),
        Some(json!({"area_id": area["id"], "code": "WC1", "name": "Lathe"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(wc["code"], "WC1");

    teardown(ctx).await;
}
