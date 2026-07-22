//! Work-order and operation lifecycles (§7 Execution, §12 M3) — pure domain.
//!
//! The valid state transitions live here so both the API handlers and tests
//! agree on one definition; illegal transitions are rejected before any DB
//! write. No I/O.

use serde::{Deserialize, Serialize};

/// Work-order lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WoStatus {
    Draft,
    Released,
    InProgress,
    Completed,
    Closed,
    Cancelled,
}

impl WoStatus {
    /// Parse the DB/wire string form.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "draft" => Self::Draft,
            "released" => Self::Released,
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            "closed" => Self::Closed,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }

    /// The DB/wire string form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Released => "released",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Whether the order is in a terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Closed | Self::Cancelled)
    }

    /// Whether a transition to `next` is allowed.
    pub fn can_transition(self, next: WoStatus) -> bool {
        use WoStatus::*;
        matches!(
            (self, next),
            (Draft, Released)
                | (Draft, Cancelled)
                | (Released, InProgress)
                | (Released, Cancelled)
                | (InProgress, Completed)
                | (InProgress, Cancelled)
                | (Completed, Closed)
        )
    }
}

/// Operation lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpStatus {
    Pending,
    InProgress,
    Completed,
}

impl OpStatus {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => Self::Pending,
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    pub fn can_transition(self, next: OpStatus) -> bool {
        use OpStatus::*;
        matches!(
            (self, next),
            (Pending, InProgress) | (InProgress, Completed)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wo_happy_path_transitions() {
        assert!(WoStatus::Draft.can_transition(WoStatus::Released));
        assert!(WoStatus::Released.can_transition(WoStatus::InProgress));
        assert!(WoStatus::InProgress.can_transition(WoStatus::Completed));
        assert!(WoStatus::Completed.can_transition(WoStatus::Closed));
    }

    #[test]
    fn wo_illegal_transitions_rejected() {
        assert!(!WoStatus::Draft.can_transition(WoStatus::InProgress));
        assert!(!WoStatus::Completed.can_transition(WoStatus::InProgress));
        assert!(!WoStatus::Closed.can_transition(WoStatus::Released));
        assert!(!WoStatus::Cancelled.can_transition(WoStatus::Released));
    }

    #[test]
    fn wo_cancel_from_active_states() {
        assert!(WoStatus::Draft.can_transition(WoStatus::Cancelled));
        assert!(WoStatus::Released.can_transition(WoStatus::Cancelled));
        assert!(WoStatus::InProgress.can_transition(WoStatus::Cancelled));
        // Cannot cancel a completed order — it must close.
        assert!(!WoStatus::Completed.can_transition(WoStatus::Cancelled));
    }

    #[test]
    fn op_transitions() {
        assert!(OpStatus::Pending.can_transition(OpStatus::InProgress));
        assert!(OpStatus::InProgress.can_transition(OpStatus::Completed));
        assert!(!OpStatus::Pending.can_transition(OpStatus::Completed));
        assert!(!OpStatus::Completed.can_transition(OpStatus::InProgress));
    }

    #[test]
    fn string_roundtrip() {
        for s in [
            "draft",
            "released",
            "in_progress",
            "completed",
            "closed",
            "cancelled",
        ] {
            assert_eq!(WoStatus::parse(s).unwrap().as_str(), s);
        }
        assert!(WoStatus::parse("bogus").is_none());
    }
}
