//! `manual` — the supervisor's "Send Diagnostics" button (§8.5). Bundles a note
//! plus a snapshot of recent buffered events; the whole bundle is redacted
//! before it leaves the box.

use serde_json::{json, Value};

/// Build a manual diagnostic report. `recent` is a `DiagBuffer` snapshot
/// (already redacted). The returned value is redacted again by the send path.
pub fn report(service: &str, version: &str, note: &str, recent: &[Value]) -> Value {
    json!({
        "event": "manual",
        "service": service,
        "version": version,
        "message": note,
        "spans": recent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_report_shape() {
        let r = report("mes-edge", "0.1.0", "operator says machine stalls", &[]);
        assert_eq!(r["event"], "manual");
        assert_eq!(r["message"], "operator says machine stalls");
    }
}
