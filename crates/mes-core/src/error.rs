//! Domain error type for `mes-core`.
//!
//! Per §14, every crate carries its own `thiserror` enum. `mes-core` is
//! I/O-free, so these variants describe *domain* invariant violations only —
//! never transport, DB, or filesystem failures.

use thiserror::Error;

/// Errors raised by pure-domain calculations and state transitions.
#[derive(Debug, Error)]
pub enum CoreError {
    /// A state-machine transition was requested that the current state forbids.
    #[error("invalid state transition: {0}")]
    InvalidTransition(String),

    /// A value fell outside the range the domain permits (e.g. negative count).
    #[error("value out of range: {0}")]
    OutOfRange(String),
}

/// Convenience alias used across the crate.
pub type CoreResult<T> = Result<T, CoreError>;
