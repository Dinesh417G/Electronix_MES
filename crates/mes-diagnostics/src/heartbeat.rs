//! `heartbeat` — a scheduled liveness ping carrying only structural identity
//! (§8.5). No business data by construction.

use serde_json::{json, Value};

/// Build a heartbeat payload. Structural only; safe to ship as-is.
pub fn payload(service: &str, version: &str, uptime_secs: u64) -> Value {
    json!({
        "event": "heartbeat",
        "service": service,
        "version": version,
        "uptime_secs": uptime_secs,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_is_structural() {
        let p = payload("mes-edge", "0.1.0", 3600);
        assert_eq!(p["event"], "heartbeat");
        assert_eq!(p["service"], "mes-edge");
        assert_eq!(p["uptime_secs"], 3600);
    }
}
