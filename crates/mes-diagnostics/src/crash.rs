//! `crash` — capture a panic as a structural report (§8.5). The panic *message*
//! can contain anything, so it is scrubbed by the send path's redaction; here we
//! keep only the location and a hash of the message for de-duplication.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde_json::{json, Value};

/// Build a crash report from a panic message + optional location. The raw
/// message is **not** included — only its hash (for grouping) and the location.
pub fn report(service: &str, version: &str, message: &str, location: Option<&str>) -> Value {
    json!({
        "event": "crash",
        "service": service,
        "version": version,
        "panic": true,
        "level": "error",
        "location": location,
        "backtrace_hash": hash(message),
    })
}

fn hash(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Install a panic hook that turns panics into crash reports handed to `sink`.
/// The previous hook is chained so normal panic output still happens.
pub fn install_hook<F>(service: &'static str, version: &'static str, sink: F)
where
    F: Fn(Value) + Send + Sync + 'static,
{
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()));
        let message = info.to_string();
        sink(report(service, version, &message, location.as_deref()));
        prev(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_report_hides_message() {
        let r = report(
            "mes-edge",
            "0.1.0",
            "panicked at part PN-SECRET",
            Some("main.rs:10"),
        );
        assert_eq!(r["event"], "crash");
        assert_eq!(r["location"], "main.rs:10");
        // The raw panic message never appears — only a hash.
        let text = serde_json::to_string(&r).unwrap();
        assert!(!text.contains("PN-SECRET"));
        assert!(r["backtrace_hash"].as_str().unwrap().len() == 16);
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(hash("same"), hash("same"));
        assert_ne!(hash("a"), hash("b"));
    }
}
