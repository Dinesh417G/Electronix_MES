//! WebSocket event contract (§10 `/ws`).
//!
//! The edge broadcasts these to connected kiosk/supervisor clients as JSON text
//! frames, tagged by `event`. The kiosk's scripted chat panel and the live
//! dashboards render them (§11).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A live event pushed to `/ws` subscribers.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WsEvent {
    /// A work order changed lifecycle status.
    WorkOrderStatusChanged {
        work_order_id: String,
        status: String,
    },
    /// An operation started.
    OperationStarted {
        work_order_id: String,
        wo_operation_id: String,
    },
    /// An operation completed.
    OperationCompleted {
        work_order_id: String,
        wo_operation_id: String,
    },
    /// Good/scrap counts were recorded against an operation.
    CountRecorded {
        wo_operation_id: String,
        good: i32,
        scrap: i32,
    },
    /// A downtime event was classified with a reason.
    DowntimeClassified {
        downtime_event_id: String,
        reason_id: String,
    },
    /// A DNC transfer was scheduled — the kiosk shows "Job ready, fetch program"
    /// (scripted, offline, no LLM call; §8.4, §11).
    DncTransferScheduled {
        transfer_id: String,
        program_id: String,
        program_identifier: String,
        wo_operation_id: Option<String>,
    },
    /// A DNC transfer completed — the kiosk clears the fetch prompt (§8.4).
    DncTransferCompleted { transfer_id: String },
    /// An operator-edited program came back as a draft revision — the supervisor
    /// console shows it in the review queue (§8.4). **Never auto-promoted** (§3).
    ProgramRevisionDrafted {
        revision_id: String,
        program_id: String,
    },
}
