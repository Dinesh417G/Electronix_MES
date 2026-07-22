//! `mes-sync` — offline-first sync plumbing (§8.3, M12).
//!
//! The edge writes an outbox row in the *same transaction* as every syncable
//! write, then pushes batches (≤500) to the cloud, which applies them
//! idempotently via `applied_entries` (§8.3). Edge is the source of truth; the
//! protocol is resumable after 24h+ offline. M0 lands only the crate shell.

#![forbid(unsafe_code)]

/// Maximum number of outbox entries pushed in a single sync batch (§8.3).
pub const MAX_BATCH: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("sync protocol error: {0}")]
    Protocol(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_cap_matches_spec() {
        assert_eq!(MAX_BATCH, 500);
    }
}
