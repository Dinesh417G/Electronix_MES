//! `/v1/master` — CRUD for equipment, products, and users (§10, §12 M1).
//!
//! Reads require any authenticated user (`AuthUser`); writes require the
//! master-write role gate (`MasterWriter` → Admin/Planner). Every mutation
//! writes an audit entry.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use mes_client::master::{
    Area, AreaInput, Part, PartInput, Program, ProgramInput, Site, SiteInput, User, UserInput,
    WorkCenter, WorkCenterInput,
};
use mes_db::{repo, repo_orders};

use crate::api::{audit, err, repo_err, require_pool, ApiErr};
use crate::auth::hash_secret;
use crate::extract::{AuthUser, MasterWriter};
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sites", get(list_sites).post(create_site))
        .route(
            "/sites/:id",
            get(get_site).put(update_site).delete(delete_site),
        )
        .route("/areas", get(list_areas).post(create_area))
        .route(
            "/areas/:id",
            get(get_area).put(update_area).delete(delete_area),
        )
        .route(
            "/work-centers",
            get(list_work_centers).post(create_work_center),
        )
        .route(
            "/work-centers/:id",
            get(get_work_center)
                .put(update_work_center)
                .delete(delete_work_center),
        )
        .route("/parts", get(list_parts).post(create_part))
        .route(
            "/parts/:id",
            get(get_part).put(update_part).delete(delete_part),
        )
        .route("/users", get(list_users).post(create_user))
        .route("/programs", get(list_programs).post(create_program))
}

// ---- Programs (routing_op ↔ DNC library, §7/§8.4) ------------------------

async fn create_program(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Json(input): Json<ProgramInput>,
) -> Result<(StatusCode, Json<Program>), ApiErr> {
    let pool = require_pool(&state)?;
    let program = repo_orders::create_program(pool, &input)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "create",
        "program",
        Some(&program.id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(program)))
}

async fn list_programs(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<Program>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_orders::list_programs(pool).await.map_err(repo_err)?,
    ))
}

// ---- Sites ---------------------------------------------------------------

async fn create_site(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Json(input): Json<SiteInput>,
) -> Result<(StatusCode, Json<Site>), ApiErr> {
    let pool = require_pool(&state)?;
    let tz = input.timezone.unwrap_or_else(|| "Asia/Kolkata".to_string());
    let site = repo::create_site(pool, &input.code, &input.name, &tz)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "create",
        "site",
        Some(&site.id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(site)))
}

async fn list_sites(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<Site>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo::list_sites(pool).await.map_err(repo_err)?))
}

async fn get_site(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Site>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo::get_site(pool, &id).await.map_err(repo_err)?))
}

async fn update_site(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
    Json(input): Json<SiteInput>,
) -> Result<Json<Site>, ApiErr> {
    let pool = require_pool(&state)?;
    let tz = input.timezone.unwrap_or_else(|| "Asia/Kolkata".to_string());
    let site = repo::update_site(pool, &id, &input.code, &input.name, &tz)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "update",
        "site",
        Some(&id),
        None,
    )
    .await;
    Ok(Json(site))
}

async fn delete_site(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiErr> {
    let pool = require_pool(&state)?;
    repo::delete_site(pool, &id).await.map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "delete",
        "site",
        Some(&id),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Areas ---------------------------------------------------------------

async fn create_area(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Json(input): Json<AreaInput>,
) -> Result<(StatusCode, Json<Area>), ApiErr> {
    let pool = require_pool(&state)?;
    let area = repo::create_area(pool, &input.site_id, &input.code, &input.name)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "create",
        "area",
        Some(&area.id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(area)))
}

async fn list_areas(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<Area>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo::list_areas(pool).await.map_err(repo_err)?))
}

async fn get_area(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Area>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo::get_area(pool, &id).await.map_err(repo_err)?))
}

async fn update_area(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
    Json(input): Json<AreaInput>,
) -> Result<Json<Area>, ApiErr> {
    let pool = require_pool(&state)?;
    let area = repo::update_area(pool, &id, &input.code, &input.name)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "update",
        "area",
        Some(&id),
        None,
    )
    .await;
    Ok(Json(area))
}

async fn delete_area(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiErr> {
    let pool = require_pool(&state)?;
    repo::delete_area(pool, &id).await.map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "delete",
        "area",
        Some(&id),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Work centers --------------------------------------------------------

async fn create_work_center(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Json(input): Json<WorkCenterInput>,
) -> Result<(StatusCode, Json<WorkCenter>), ApiErr> {
    let pool = require_pool(&state)?;
    let wc = repo::create_work_center(
        pool,
        &input.area_id,
        &input.code,
        &input.name,
        input.external_ref.as_deref(),
    )
    .await
    .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "create",
        "work_center",
        Some(&wc.id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(wc)))
}

async fn list_work_centers(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<WorkCenter>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo::list_work_centers(pool).await.map_err(repo_err)?))
}

async fn get_work_center(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<WorkCenter>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo::get_work_center(pool, &id).await.map_err(repo_err)?,
    ))
}

async fn update_work_center(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
    Json(input): Json<WorkCenterInput>,
) -> Result<Json<WorkCenter>, ApiErr> {
    let pool = require_pool(&state)?;
    let wc = repo::update_work_center(
        pool,
        &id,
        &input.code,
        &input.name,
        input.external_ref.as_deref(),
    )
    .await
    .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "update",
        "work_center",
        Some(&id),
        None,
    )
    .await;
    Ok(Json(wc))
}

async fn delete_work_center(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiErr> {
    let pool = require_pool(&state)?;
    repo::delete_work_center(pool, &id)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "delete",
        "work_center",
        Some(&id),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Parts ---------------------------------------------------------------

async fn create_part(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Json(input): Json<PartInput>,
) -> Result<(StatusCode, Json<Part>), ApiErr> {
    let pool = require_pool(&state)?;
    let uom = input.uom.unwrap_or_else(|| "ea".to_string());
    let part = repo::create_part(pool, &input.code, &input.name, &uom)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "create",
        "part",
        Some(&part.id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(part)))
}

async fn list_parts(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<Part>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo::list_parts(pool).await.map_err(repo_err)?))
}

async fn get_part(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Part>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo::get_part(pool, &id).await.map_err(repo_err)?))
}

async fn update_part(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
    Json(input): Json<PartInput>,
) -> Result<Json<Part>, ApiErr> {
    let pool = require_pool(&state)?;
    let uom = input.uom.unwrap_or_else(|| "ea".to_string());
    let part = repo::update_part(pool, &id, &input.code, &input.name, &uom)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "update",
        "part",
        Some(&id),
        None,
    )
    .await;
    Ok(Json(part))
}

async fn delete_part(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiErr> {
    let pool = require_pool(&state)?;
    repo::delete_part(pool, &id).await.map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "delete",
        "part",
        Some(&id),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Users ---------------------------------------------------------------

async fn create_user(
    State(state): State<AppState>,
    MasterWriter(actor): MasterWriter,
    Json(input): Json<UserInput>,
) -> Result<(StatusCode, Json<User>), ApiErr> {
    let pool = require_pool(&state)?;

    let password_hash = match input.password.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => Some(
            hash_secret(p)
                .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "hash error"))?,
        ),
        None => None,
    };
    let pin_hash = match input.pin.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => Some(
            hash_secret(p)
                .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "hash error"))?,
        ),
        None => None,
    };

    let user = repo::create_user(
        pool,
        &input.username,
        &input.display_name,
        &input.role_code,
        password_hash.as_deref(),
        pin_hash.as_deref(),
        input.badge_id.as_deref().filter(|b| !b.is_empty()),
    )
    .await
    .map_err(repo_err)?;
    audit(
        pool,
        Some(&actor.user_id),
        "create",
        "user",
        Some(&user.id),
        None,
    )
    .await;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn list_users(
    State(state): State<AppState>,
    _writer: MasterWriter,
) -> Result<Json<Vec<User>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(repo::list_users(pool).await.map_err(repo_err)?))
}
