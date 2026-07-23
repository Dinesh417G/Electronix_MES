//! `buffer` — a bounded ring of recent diagnostic events, redacted on the way in
//! so the buffer itself never holds business data (§8.5). A manual/crash report
//! attaches a snapshot for context.

use std::collections::VecDeque;

use serde_json::Value;

use crate::redact::redact;

/// A fixed-capacity ring buffer of already-redacted events.
#[derive(Debug)]
pub struct DiagBuffer {
    cap: usize,
    events: VecDeque<Value>,
}

impl DiagBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            events: VecDeque::with_capacity(cap.max(1)),
        }
    }

    /// Push an event, redacting it first. Oldest is evicted past capacity.
    pub fn push(&mut self, event: &Value) {
        if self.events.len() == self.cap {
            self.events.pop_front();
        }
        self.events.push_back(redact(event));
    }

    /// A snapshot of the (already-redacted) recent events, oldest first.
    pub fn snapshot(&self) -> Vec<Value> {
        self.events.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn evicts_oldest_and_redacts() {
        let mut b = DiagBuffer::new(2);
        b.push(&json!({ "event": "a", "part_number": "PN-1" }));
        b.push(&json!({ "event": "b" }));
        b.push(&json!({ "event": "c" }));
        let snap = b.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0]["event"], "b");
        assert_eq!(snap[1]["event"], "c");
        // The evicted-or-not, business key never stored.
        assert!(serde_json::to_string(&snap).unwrap().find("PN-1").is_none());
    }
}
