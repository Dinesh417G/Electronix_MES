//! `error_trigger` — a report raised automatically on a critical error (§8.5).
//! Carries the error's structure, not the data that produced it.

use serde_json::{json, Value};

/// Build an error-triggered report. `error` is a message (string-scrubbed by the
/// send path) and `error_type` a stable classifier.
pub fn report(service: &str, version: &str, error_type: &str, error: &str) -> Value {
    json!({
        "event": "error_trigger",
        "service": service,
        "version": version,
        "error_type": error_type,
        "error": error,
        "level": "error",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_report_shape() {
        let r = report("mes-cloud", "0.1.0", "db_pool_exhausted", "no connections");
        assert_eq!(r["event"], "error_trigger");
        assert_eq!(r["error_type"], "db_pool_exhausted");
        assert_eq!(r["level"], "error");
    }
}
