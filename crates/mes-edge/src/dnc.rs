//! `/v1/dnc` + DNC orchestration (§8.4, §10, §12 M4).
//!
//! The business flow that sits on top of the `mes-dnc-bridge` transport: when a
//! job completes, resolve the next operation's program and ask the daemon to
//! stage the transfer, recording a `dnc_transfer_events` row and notifying the
//! kiosk. Daemon events mark transfers complete (clearing the kiosk prompt) or
//! turn an edited program into a **draft** `program_revisions` row — never
//! auto-promoted (§3). Supervisors promote/reject from the review queue.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mes_client::dnc::{ManualTransferInput, ProgramRevision, TransferEvent};
use mes_client::ws::WsEvent;
use mes_core::dnc::{RevisionStatus, TransferDirection, TransferStatus};
use mes_db::repo_dnc;
use mes_dnc_bridge::{DncCommand, DncEvent};

use crate::api::{audit, err, repo_err, require_pool, ApiErr};
use crate::extract::AuthUser;
use crate::http::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/transfers", get(list_transfers).post(manual_transfer))
        .route("/transfers/:id/retry", post(retry_transfer))
        .route("/revisions", get(list_revisions))
        .route("/revisions/:id/promote", post(promote_revision))
        .route("/revisions/:id/reject", post(reject_revision))
        // Test/integration hook: inject a simulated daemon event (§13).
        .route("/daemon-events", post(inject_daemon_event))
}

// ===========================================================================
// Orchestration (called from exec on job completion, and from the endpoints)
// ===========================================================================

/// Ask the daemon to stage a transfer of `program_id` to its machine, record a
/// Scheduled `dnc_transfer_events` row, and notify the kiosk. Returns the row.
pub async fn schedule_transfer(
    state: &AppState,
    program_id: &str,
    program_identifier: &str,
    machine: Option<String>,
    wo_operation_id: Option<&str>,
) -> Result<TransferEvent, ApiErr> {
    let pool = require_pool(state)?;

    let daemon_ref = state
        .dnc
        .send(DncCommand::SendProgram {
            program: program_identifier.to_string(),
            machine,
        })
        .await
        .map_err(|e| {
            err(
                StatusCode::BAD_GATEWAY,
                "dnc_unavailable",
                format!("dnc-daemon rejected the command: {e}"),
            )
        })?;

    let transfer = repo_dnc::create_transfer(
        pool,
        wo_operation_id,
        program_id,
        TransferDirection::ToMachine.as_str(),
        Some(&daemon_ref),
    )
    .await
    .map_err(repo_err)?;

    state.publish(WsEvent::DncTransferScheduled {
        transfer_id: transfer.id.clone(),
        program_id: program_id.to_string(),
        program_identifier: program_identifier.to_string(),
        wo_operation_id: wo_operation_id.map(str::to_string),
    });
    Ok(transfer)
}

/// Best-effort auto-schedule after an operation completes (§8.4). Resolves the
/// next queued operation's program; if there's nothing to transfer, does
/// nothing. Errors are logged, not propagated — completing the op must not fail
/// because the CNC pipeline is unavailable.
pub async fn on_job_complete(state: &AppState, completed_op_id: &str) {
    let Ok(pool) = require_pool(state) else {
        return;
    };
    match repo_dnc::resolve_next_transfer(pool, completed_op_id).await {
        Ok(Some(next)) => {
            if let Err(e) = schedule_transfer(
                state,
                &next.program_id,
                &next.program_identifier,
                next.target_machine,
                Some(&next.next_op_id),
            )
            .await
            {
                tracing::warn!(?e, "auto-schedule of DNC transfer failed");
            }
        }
        Ok(None) => {}
        Err(e) => tracing::warn!(error = %e, "resolving next DNC transfer failed"),
    }
}

/// Apply a daemon event to MES state (§8.4 steps 4–5).
pub async fn handle_daemon_event(state: &AppState, event: DncEvent) -> Result<(), ApiErr> {
    let pool = require_pool(state)?;
    match event {
        DncEvent::TransferCompleted { reference } => {
            if let Some(t) = repo_dnc::find_open_transfer_by_ref(pool, &reference)
                .await
                .map_err(repo_err)?
            {
                repo_dnc::set_transfer_status(pool, &t.id, TransferStatus::Completed.as_str())
                    .await
                    .map_err(repo_err)?;
                state.publish(WsEvent::DncTransferCompleted { transfer_id: t.id });
            }
        }
        DncEvent::TransferFailed { reference, .. } => {
            if let Some(t) = repo_dnc::find_open_transfer_by_ref(pool, &reference)
                .await
                .map_err(repo_err)?
            {
                repo_dnc::set_transfer_status(pool, &t.id, TransferStatus::Failed.as_str())
                    .await
                    .map_err(repo_err)?;
            }
        }
        DncEvent::ProgramReceived {
            program,
            content_ref,
        } => {
            // Operator edited the program at the machine → a DRAFT revision.
            if let Some(program_id) = repo_dnc::find_program_id_by_identifier(pool, &program)
                .await
                .map_err(repo_err)?
            {
                let rev = repo_dnc::create_draft_revision(
                    pool,
                    &program_id,
                    "operator_edit",
                    content_ref.as_deref(),
                    None,
                )
                .await
                .map_err(repo_err)?;
                state.publish(WsEvent::ProgramRevisionDrafted {
                    revision_id: rev.id,
                    program_id,
                });
            } else {
                tracing::warn!(program, "received edit for unknown program; dropping");
            }
        }
    }
    Ok(())
}

// ===========================================================================
// HTTP handlers
// ===========================================================================

async fn list_transfers(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<TransferEvent>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_dnc::list_transfers(pool).await.map_err(repo_err)?,
    ))
}

async fn manual_transfer(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(input): Json<ManualTransferInput>,
) -> Result<(StatusCode, Json<TransferEvent>), ApiErr> {
    let pool = require_pool(&state)?;
    let (identifier, machine) = repo_dnc::program_dispatch_info(pool, &input.program_id)
        .await
        .map_err(repo_err)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "not_found", "program not found"))?;
    let machine = input.machine.or(machine);
    let transfer = schedule_transfer(
        &state,
        &input.program_id,
        &identifier,
        machine,
        input.wo_operation_id.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(transfer)))
}

async fn retry_transfer(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<TransferEvent>), ApiErr> {
    let pool = require_pool(&state)?;
    let existing = repo_dnc::get_transfer(pool, &id).await.map_err(repo_err)?;
    let (identifier, machine) = repo_dnc::program_dispatch_info(pool, &existing.program_id)
        .await
        .map_err(repo_err)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "not_found", "program not found"))?;
    let transfer = schedule_transfer(
        &state,
        &existing.program_id,
        &identifier,
        machine,
        existing.wo_operation_id.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(transfer)))
}

async fn list_revisions(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Vec<ProgramRevision>>, ApiErr> {
    let pool = require_pool(&state)?;
    Ok(Json(
        repo_dnc::list_revisions(pool).await.map_err(repo_err)?,
    ))
}

/// Promote or reject a draft revision — Supervisor/Admin/Planner only (§8.4).
async fn transition_revision(
    state: &AppState,
    auth: &AuthUser,
    id: &str,
    target: RevisionStatus,
) -> Result<ProgramRevision, ApiErr> {
    if !mes_core::roles::can_promote_revision(&auth.role) {
        return Err(err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "role may not review program revisions",
        ));
    }
    let pool = require_pool(state)?;
    let rev = repo_dnc::get_revision(pool, id).await.map_err(repo_err)?;
    let current = RevisionStatus::parse(&rev.status)
        .ok_or_else(|| err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "bad status"))?;
    if !current.can_transition(target) {
        return Err(err(
            StatusCode::CONFLICT,
            "invalid_transition",
            format!(
                "revision cannot move from {} to {}",
                current.as_str(),
                target.as_str()
            ),
        ));
    }
    let updated = repo_dnc::set_revision_status(pool, id, target.as_str(), &auth.user_id)
        .await
        .map_err(repo_err)?;
    audit(
        pool,
        Some(&auth.user_id),
        target.as_str(),
        "program_revision",
        Some(id),
        None,
    )
    .await;
    Ok(updated)
}

async fn promote_revision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<ProgramRevision>, ApiErr> {
    Ok(Json(
        transition_revision(&state, &auth, &id, RevisionStatus::Promoted).await?,
    ))
}

async fn reject_revision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<ProgramRevision>, ApiErr> {
    Ok(Json(
        transition_revision(&state, &auth, &id, RevisionStatus::Rejected).await?,
    ))
}

/// Feed a simulated daemon event through the orchestration path. This is the
/// seam the `machine-sim` virtual dnc-daemon and the M4 tests drive (§13); a
/// real deployment's daemon event loop calls [`handle_daemon_event`] directly.
async fn inject_daemon_event(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(event): Json<DncEvent>,
) -> Result<StatusCode, ApiErr> {
    handle_daemon_event(&state, event).await?;
    Ok(StatusCode::ACCEPTED)
}
