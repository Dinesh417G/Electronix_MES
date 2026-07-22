//! Role codes and authorization policy (pure domain).
//!
//! `roles` is a DB lookup table (§7), but the *policy* — which role may do what
//! — is domain logic and lives here so it is unit-testable with no I/O and has
//! exactly one definition. Handlers consult these helpers; they never hard-code
//! role strings inline.

/// Seeded role codes (§7). Maintenance is added at M9 as a plain row; when it
/// exists it has no master-data write rights unless added here deliberately.
pub const ADMIN: &str = "Admin";
pub const PLANNER: &str = "Planner";
pub const SUPERVISOR: &str = "Supervisor";
pub const OPERATOR: &str = "Operator";
pub const QUALITY: &str = "Quality";

/// May the given role create/update/delete master data (equipment, products,
/// people)? Admin and Planner only — Operators explicitly cannot (§12 M1
/// acceptance: "Operator cannot touch master data").
pub fn can_write_master(role_code: &str) -> bool {
    matches!(role_code, ADMIN | PLANNER)
}

/// May the given role read master data? Every authenticated role may read.
pub fn can_read_master(_role_code: &str) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_admin_and_planner_write_master() {
        assert!(can_write_master(ADMIN));
        assert!(can_write_master(PLANNER));
        assert!(!can_write_master(SUPERVISOR));
        assert!(!can_write_master(OPERATOR));
        assert!(!can_write_master(QUALITY));
    }

    #[test]
    fn unknown_role_cannot_write_master() {
        assert!(!can_write_master("Maintenance"));
        assert!(!can_write_master(""));
    }

    #[test]
    fn everyone_reads_master() {
        assert!(can_read_master(OPERATOR));
    }
}
