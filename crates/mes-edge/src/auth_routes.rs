//! `/v1/auth` — console (password) and kiosk (PIN/badge) login (§10, §12 M1).

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use mes_client::auth::{LoginRequest, LoginResponse, PinLoginRequest};
use mes_db::repo::{self, UserAuth};

use crate::api::{audit, err, require_pool, ApiErr};
use crate::auth::{issue_token, verify_secret};
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/pin-login", post(pin_login))
}

/// Generic 401 — never reveals whether the username, password, or account state
/// was the problem, to avoid user enumeration.
fn unauthorized() -> ApiErr {
    err(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "invalid credentials",
    )
}

async fn finish_login(
    state: &AppState,
    user: UserAuth,
    method: &str,
) -> Result<Json<LoginResponse>, ApiErr> {
    let (token, expires_at) = issue_token(&state.auth, &user.id, &user.role_code)
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "token error"))?;

    if let Ok(pool) = require_pool(state) {
        audit(
            pool,
            Some(&user.id),
            "login",
            "user",
            Some(&user.id),
            Some(serde_json::json!({ "method": method })),
        )
        .await;
    }

    Ok(Json(LoginResponse {
        token,
        user_id: user.id,
        username: user.username,
        role_code: user.role_code,
        expires_at,
    }))
}

/// Console/password login.
async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiErr> {
    let pool = require_pool(&state)?;
    let user = match repo::find_auth_by_username(pool, &body.username).await {
        Ok(u) => u,
        Err(repo::RepoError::NotFound) => return Err(unauthorized()),
        Err(e) => return Err(crate::api::repo_err(e)),
    };

    let ok = user.active
        && user
            .password_hash
            .as_deref()
            .is_some_and(|h| verify_secret(&body.password, h));
    if !ok {
        return Err(unauthorized());
    }

    finish_login(&state, user, "password").await
}

/// Kiosk login: badge presence, or username + PIN.
async fn pin_login(
    State(state): State<AppState>,
    Json(body): Json<PinLoginRequest>,
) -> Result<Json<LoginResponse>, ApiErr> {
    let pool = require_pool(&state)?;

    // Badge scan authenticates by presence (kiosk terminal is physically trusted).
    if let Some(badge) = body.badge_id.as_deref().filter(|b| !b.is_empty()) {
        let user = match repo::find_auth_by_badge(pool, badge).await {
            Ok(u) => u,
            Err(repo::RepoError::NotFound) => return Err(unauthorized()),
            Err(e) => return Err(crate::api::repo_err(e)),
        };
        if !user.active {
            return Err(unauthorized());
        }
        return finish_login(&state, user, "badge").await;
    }

    // Otherwise require username + PIN.
    match (body.username.as_deref(), body.pin.as_deref()) {
        (Some(username), Some(pin)) if !username.is_empty() && !pin.is_empty() => {
            let user = match repo::find_auth_by_username(pool, username).await {
                Ok(u) => u,
                Err(repo::RepoError::NotFound) => return Err(unauthorized()),
                Err(e) => return Err(crate::api::repo_err(e)),
            };
            let ok = user.active
                && user
                    .pin_hash
                    .as_deref()
                    .is_some_and(|h| verify_secret(pin, h));
            if !ok {
                return Err(unauthorized());
            }
            finish_login(&state, user, "pin").await
        }
        _ => Err(err(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "provide badge_id, or username and pin",
        )),
    }
}
