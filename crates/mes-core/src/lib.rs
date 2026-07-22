//! `mes-core` — pure domain logic for ElectronIx MES.
//!
//! This crate holds types, the machine state machine, OEE math, and shift/PM-due
//! calculations (§6, §8). It performs **no I/O**: no DB pool, no sockets, no
//! filesystem. Everything here is deterministic and unit-testable, which keeps
//! the correctness-critical maths (§8.2, §13) verifiable in isolation.
//!
//! M0 establishes the crate shape and shared primitives; milestones M1+ append
//! domain modules without rewriting what is here.

#![forbid(unsafe_code)]

pub mod dnc;
pub mod error;
pub mod id;
pub mod roles;
pub mod state_machine;
pub mod work_order;

pub use error::{CoreError, CoreResult};
pub use id::new_id;

/// Semantic version of the domain crate, surfaced by services in `/healthz`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_populated() {
        assert!(!VERSION.is_empty());
    }
}
