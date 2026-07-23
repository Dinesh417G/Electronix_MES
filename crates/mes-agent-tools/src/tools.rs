//! The read-only tool implementations (§8.6). Every query is scoped to the
//! caller's org through `plants.plant_id`; none can be widened by the caller.
//!
//! In v1 only work orders are aggregated to the cloud (§12 M12), so
//! `get_wo_status` returns live tenant-scoped data; the other tools are present,
//! tenant-scoped, and return an empty result with a note until their aggregate
//! is synced to the cloud. Adding data to them later is additive — no signature
//! or transport change.

use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{AgentToolError, TenantScope, ToolDef};

fn qerr(e: sqlx::Error) -> AgentToolError {
    AgentToolError::Query(e.to_string())
}

/// Work-order status for the tenant's plants (live tenant-scoped data).
pub async fn get_wo_status(
    pool: &PgPool,
    scope: &TenantScope,
    args: &Value,
) -> Result<Value, AgentToolError> {
    let status_filter = args.get("status").and_then(Value::as_str);

    let rows: Vec<(String, String, Decimal, Option<String>)> = sqlx::query_as(
        "SELECT w.wo_number, w.status, w.qty_ordered, w.plant_id
         FROM work_orders w
         JOIN plants p ON p.id = w.plant_id
         WHERE p.org_id = $1
           AND ($2::text IS NULL OR w.status = $2)
         ORDER BY w.created_at DESC
         LIMIT 200",
    )
    .bind(&scope.org_id)
    .bind(status_filter)
    .fetch_all(pool)
    .await
    .map_err(qerr)?;

    let mut counts: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let items: Vec<Value> = rows
        .iter()
        .map(|(wo_number, status, qty, plant_id)| {
            *counts.entry(status.clone()).or_default() += 1;
            json!({ "wo_number": wo_number, "status": status, "qty_ordered": qty, "plant_id": plant_id })
        })
        .collect();

    Ok(json!({
        "total": items.len(),
        "counts_by_status": counts,
        "work_orders": items,
    }))
}

/// A tenant-scoped tool whose source aggregate is not yet synced to the cloud.
fn pending(aggregate: &str) -> Value {
    json!({
        "items": [],
        "note": format!(
            "{aggregate} is available per-plant on the edge; cloud aggregation for this \
             tool arrives when that aggregate is added to the sync set (additive)."
        ),
    })
}

pub async fn get_oee(
    _pool: &PgPool,
    _scope: &TenantScope,
    _args: &Value,
) -> Result<Value, AgentToolError> {
    Ok(pending("OEE"))
}

pub async fn get_downtime_pareto(
    _pool: &PgPool,
    _scope: &TenantScope,
    _args: &Value,
) -> Result<Value, AgentToolError> {
    Ok(pending("Downtime Pareto"))
}

pub async fn get_ncr_queue(
    _pool: &PgPool,
    _scope: &TenantScope,
    _args: &Value,
) -> Result<Value, AgentToolError> {
    Ok(pending("The NCR queue"))
}

pub async fn get_trace(
    _pool: &PgPool,
    _scope: &TenantScope,
    _args: &Value,
) -> Result<Value, AgentToolError> {
    Ok(pending("Traceability"))
}

pub async fn get_maintenance_due(
    _pool: &PgPool,
    _scope: &TenantScope,
    _args: &Value,
) -> Result<Value, AgentToolError> {
    Ok(pending("Maintenance-due"))
}

/// The read-only tool catalog (§8.6) — advertised by both front doors.
pub fn catalog() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_wo_status",
            description: "Work-order status and counts for the caller's plants. \
                          Optional `status` filters to one lifecycle status.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "status": { "type": "string", "description": "Filter to a WO status" }
                }
            }),
        },
        ToolDef {
            name: "get_oee",
            description: "OEE (A×P×Q) for a work center over a window.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "work_center_id": { "type": "string" },
                    "start": { "type": "string", "description": "RFC3339" },
                    "end": { "type": "string", "description": "RFC3339" }
                }
            }),
        },
        ToolDef {
            name: "get_downtime_pareto",
            description: "Ranked downtime loss by reason for a work center over a window.",
            input_schema: json!({
                "type": "object",
                "properties": { "work_center_id": { "type": "string" } }
            }),
        },
        ToolDef {
            name: "get_ncr_queue",
            description: "Open non-conformances (NCRs) for the caller's plants.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDef {
            name: "get_trace",
            description: "Forward/backward genealogy for a lot or serial.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "entity_type": { "type": "string", "enum": ["lot", "serial"] },
                    "entity_id": { "type": "string" },
                    "direction": { "type": "string", "enum": ["forward", "backward"] }
                }
            }),
        },
        ToolDef {
            name: "get_maintenance_due",
            description: "Preventive-maintenance schedules currently due.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
    ]
}
