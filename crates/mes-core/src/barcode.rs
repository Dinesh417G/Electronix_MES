//! Barcode encoding (§7, §12 M7) — the `EMX1|<type>|<id>` format.
//!
//! Pipe-delimited: a fixed `EMX1` prefix, a short type code, and the entity id.
//! Pure string logic, no I/O.

/// Type code for a lot.
pub const TYPE_LOT: &str = "LOT";
/// Type code for a serial.
pub const TYPE_SERIAL: &str = "SER";

/// The fixed format prefix.
pub const PREFIX: &str = "EMX1";

/// Encode an entity into a barcode string: `EMX1|<type>|<id>`.
pub fn encode(type_code: &str, id: &str) -> String {
    format!("{PREFIX}|{type_code}|{id}")
}

/// Parse a barcode into `(type_code, id)`, or `None` if malformed. The id may
/// itself be any non-empty string (ULIDs contain no `|`).
pub fn parse(code: &str) -> Option<(String, String)> {
    let mut parts = code.splitn(3, '|');
    let prefix = parts.next()?;
    let type_code = parts.next()?;
    let id = parts.next()?;
    if prefix != PREFIX || type_code.is_empty() || id.is_empty() {
        return None;
    }
    Some((type_code.to_string(), id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips() {
        let code = encode(TYPE_LOT, "01HXYZ");
        assert_eq!(code, "EMX1|LOT|01HXYZ");
        assert_eq!(
            parse(&code),
            Some(("LOT".to_string(), "01HXYZ".to_string()))
        );
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse("").is_none());
        assert!(parse("EMX1|LOT").is_none()); // missing id
        assert!(parse("NOPE|LOT|1").is_none()); // wrong prefix
        assert!(parse("EMX1||1").is_none()); // empty type
        assert!(parse("EMX1|LOT|").is_none()); // empty id
    }

    #[test]
    fn id_may_contain_nothing_special() {
        // splitn(3) keeps any trailing content (incl. stray pipes) in the id.
        assert_eq!(
            parse("EMX1|SER|a|b"),
            Some(("SER".to_string(), "a|b".to_string()))
        );
    }
}
