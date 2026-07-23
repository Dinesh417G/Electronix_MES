//! Redaction (§8.5) — **stricter** than DNC's. MES diagnostics can carry
//! production counts, scrap reasons, part numbers, customer names, pricing and
//! raw inspection values that DNC never touched, so redaction is an **allowlist**:
//! only structural/error keys survive; every other field is dropped. That makes
//! "did a business field leak?" impossible-by-default rather than a denylist we
//! must keep chasing.

use serde_json::{Map, Value};

/// Keys whose values are structural/diagnostic and safe to ship. Anything not on
/// this list is dropped. Deliberately conservative (§8.5).
const SAFE_KEYS: &[&str] = &[
    // service / build identity
    "service",
    "version",
    "build",
    "commit",
    "os",
    "arch",
    "cpu_count",
    "mem_bytes",
    // event / log structure
    "level",
    "target",
    "event",
    "kind",
    "code",
    "status",
    "timestamp",
    "ts",
    "uptime_secs",
    "elapsed_ms",
    "count",
    "seq",
    // error / crash structure (messages are additionally string-scrubbed)
    "error",
    "error_type",
    "message",
    "panic",
    "location",
    "thread",
    "backtrace_hash",
    "span",
    "spans",
    "reason_code",
];

/// Scrub obvious secrets embedded in an allowed free-text string: emails and
/// long opaque tokens. Not a substitute for the allowlist — a second line only.
fn scrub_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split_inclusive(char::is_whitespace) {
        let trimmed = word.trim_end();
        let ws = &word[trimmed.len()..];
        if looks_like_email(trimmed) || looks_like_token(trimmed) {
            out.push_str("[redacted]");
        } else {
            out.push_str(trimmed);
        }
        out.push_str(ws);
    }
    out
}

fn looks_like_email(w: &str) -> bool {
    let at = w.find('@');
    matches!(at, Some(i) if i > 0 && w[i + 1..].contains('.'))
}

fn looks_like_token(w: &str) -> bool {
    // 24+ chars of base64/hex-ish with no spaces → treat as an opaque secret.
    w.len() >= 24
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '+' || c == '/')
        && w.chars().any(|c| c.is_ascii_digit())
}

/// Redact a diagnostic payload: keep only allowlisted keys (recursively),
/// string-scrubbing the values that survive.
pub fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                if SAFE_KEYS.contains(&k.as_str()) {
                    out.insert(k.clone(), redact(v));
                }
                // else: dropped (business data never ships, §8.5).
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact).collect()),
        Value::String(s) => Value::String(scrub_string(s)),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A fixture stuffed with fake sensitive business fields (§13 — assert none
    /// survive redaction).
    fn sensitive_fixture() -> Value {
        json!({
            "service": "mes-edge",
            "version": "0.1.0",
            "level": "error",
            "error": "insert failed",
            "part_number": "PN-SECRET-123",
            "customer_name": "Acme Aerospace Pvt Ltd",
            "price": 148500.75,
            "measured_value": 10.037,
            "scrap_reason": "porosity in weld",
            "lot_no": "LOT-9987",
            "serial_no": "SN-ABC-0001",
            "operator_pin": "4821",
            "nested": {
                "wo_number": "WO-CONFIDENTIAL-42",
                "inspection_value": 9.98,
                "status": "fail"
            },
            "rows": [
                { "part_name": "Titanium bracket", "qty_good": 12 }
            ],
            "spans": [
                { "status": "fail", "part_number": "PN-INNER-77" }
            ]
        })
    }

    fn as_text(v: &Value) -> String {
        serde_json::to_string(v).unwrap()
    }

    #[test]
    fn no_business_field_survives_redaction() {
        let redacted = redact(&sensitive_fixture());
        let text = as_text(&redacted);

        // None of the fake sensitive values appear anywhere in the output.
        for leak in [
            "PN-SECRET-123",
            "Acme Aerospace",
            "148500",
            "10.037",
            "porosity",
            "LOT-9987",
            "SN-ABC-0001",
            "4821",
            "WO-CONFIDENTIAL-42",
            "9.98",
            "Titanium bracket",
            "PN-INNER-77",
        ] {
            assert!(!text.contains(leak), "leaked sensitive value: {leak}");
        }
    }

    #[test]
    fn structural_fields_are_kept() {
        let redacted = redact(&sensitive_fixture());
        assert_eq!(redacted["service"], "mes-edge");
        assert_eq!(redacted["version"], "0.1.0");
        assert_eq!(redacted["level"], "error");
        assert_eq!(redacted["error"], "insert failed");
        // A container under a non-safe key is dropped wholesale (allowlist, §8.5).
        assert!(redacted.get("nested").is_none());
        // A container under a *safe* key survives, but its business keys are still
        // dropped recursively: only the safe `status` remains.
        assert_eq!(redacted["spans"][0]["status"], "fail");
        assert!(redacted["spans"][0].get("part_number").is_none());
    }

    #[test]
    fn embedded_email_and_token_are_scrubbed() {
        let v = json!({ "message": "user jane@acme.com token abcdef0123456789ABCDEF9999 failed" });
        let text = as_text(&redact(&v));
        assert!(!text.contains("jane@acme.com"));
        assert!(!text.contains("abcdef0123456789ABCDEF9999"));
        assert!(text.contains("failed"));
    }

    #[test]
    fn non_safe_container_keys_are_dropped() {
        let redacted = redact(&sensitive_fixture());
        // `rows` is not an allowlisted key → dropped entirely (no business leak).
        assert!(redacted.get("rows").is_none());
    }
}
