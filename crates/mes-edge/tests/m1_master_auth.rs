//! M1 acceptance tests — master-data CRUD + role enforcement (§12 M1, §13).
//!
//! Each test runs against its own freshly-migrated Postgres schema (§13
//! "fresh schema per test"), so tests never interfere. The suite is gated on
//! `DATABASE_URL`: it runs in CI (which provides a TimescaleDB service) and is
//! skipped where no database is available.

mod common;

use axum::http::StatusCode;
use common::{call, seed_user_token, setup, teardown};
use serde_json::json;

#[tokio::test]
async fn roles_are_seeded() {
    let Some(ctx) = setup().await else {
        return;
    };
    let pool = ctx.state.pool.as_ref().unwrap();
    // The five base roles M1 seeds. Later milestones add more additively
    // (Maintenance at M9, §7), so assert the base set is present by code rather
    // than an exact total row count.
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM roles
         WHERE code IN ('Admin', 'Planner', 'Supervisor', 'Operator', 'Quality')",
    )
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
