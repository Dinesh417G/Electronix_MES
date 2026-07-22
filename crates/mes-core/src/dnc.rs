//! DNC transfer + program-revision lifecycles (§7, §8.4, §12 M4) — pure domain.
//!
//! State transitions for CNC program transfers and operator-edited program
//! revisions. Kept here (no I/O) so handlers and tests share one definition. A
//! program revision is **never auto-promoted** — a supervisor must promote it
//! (§3), which the transition table enforces by only allowing Draft→Promoted as
//! a deliberate step.

use serde::{Deserialize, Serialize};

/// Direction of a DNC transfer relative to the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    /// Program pushed to the machine.
    ToMachine,
    /// Program received back from the machine (e.g. an operator edit).
    FromMachine,
}

impl TransferDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToMachine => "to_machine",
            Self::FromMachine => "from_machine",
        }
    }
}

/// Lifecycle of a DNC transfer (§7 `dnc_transfer_events`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Scheduled,
    Notified,
    Fetched,
    Completed,
    Failed,
}

impl TransferStatus {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "scheduled" => Self::Scheduled,
            "notified" => Self::Notified,
            "fetched" => Self::Fetched,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Notified => "notified",
            Self::Fetched => "fetched",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }

    /// Allowed forward transitions. A transfer can fail from any non-terminal
    /// state, and progresses Scheduled→Notified→Fetched→Completed.
    pub fn can_transition(self, next: TransferStatus) -> bool {
        use TransferStatus::*;
        if next == Failed {
            return !self.is_terminal();
        }
        matches!(
            (self, next),
            (Scheduled, Notified)
                | (Scheduled, Fetched)
                | (Scheduled, Completed)
                | (Notified, Fetched)
                | (Notified, Completed)
                | (Fetched, Completed)
        )
    }
}

/// Lifecycle of an operator-edited program revision (§7 `program_revisions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionStatus {
    Draft,
    Promoted,
    Rejected,
}

impl RevisionStatus {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "draft" => Self::Draft,
            "promoted" => Self::Promoted,
            "rejected" => Self::Rejected,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Promoted => "promoted",
            Self::Rejected => "rejected",
        }
    }

    /// A draft may be promoted or rejected; promoted/rejected are terminal.
    pub fn can_transition(self, next: RevisionStatus) -> bool {
        use RevisionStatus::*;
        matches!((self, next), (Draft, Promoted) | (Draft, Rejected))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_happy_path() {
        assert!(TransferStatus::Scheduled.can_transition(TransferStatus::Notified));
        assert!(TransferStatus::Notified.can_transition(TransferStatus::Fetched));
        assert!(TransferStatus::Fetched.can_transition(TransferStatus::Completed));
        // A daemon may report completion directly from Scheduled.
        assert!(TransferStatus::Scheduled.can_transition(TransferStatus::Completed));
    }

    #[test]
    fn transfer_can_fail_only_when_active() {
        assert!(TransferStatus::Scheduled.can_transition(TransferStatus::Failed));
        assert!(TransferStatus::Notified.can_transition(TransferStatus::Failed));
        assert!(!TransferStatus::Completed.can_transition(TransferStatus::Failed));
        assert!(!TransferStatus::Failed.can_transition(TransferStatus::Failed));
    }

    #[test]
    fn transfer_no_backwards() {
        assert!(!TransferStatus::Completed.can_transition(TransferStatus::Scheduled));
        assert!(!TransferStatus::Fetched.can_transition(TransferStatus::Notified));
    }

    #[test]
    fn revision_promote_or_reject_only_from_draft() {
        assert!(RevisionStatus::Draft.can_transition(RevisionStatus::Promoted));
        assert!(RevisionStatus::Draft.can_transition(RevisionStatus::Rejected));
        // Never auto-promoted / never re-promoted (§3).
        assert!(!RevisionStatus::Promoted.can_transition(RevisionStatus::Rejected));
        assert!(!RevisionStatus::Rejected.can_transition(RevisionStatus::Promoted));
    }
}
