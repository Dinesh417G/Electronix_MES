//! ULID identifier helpers.
//!
//! Every table uses `id TEXT` populated with a ULID (§7). Generating them in
//! one place keeps the format consistent and lexicographically sortable by
//! creation time.

use ulid::Ulid;

/// Generate a fresh ULID as its canonical 26-character Crockford-base32 string.
pub fn new_id() -> String {
    Ulid::new().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_26_chars_and_unique() {
        let a = new_id();
        let b = new_id();
        assert_eq!(a.len(), 26);
        assert_eq!(b.len(), 26);
        assert_ne!(a, b);
    }
}
