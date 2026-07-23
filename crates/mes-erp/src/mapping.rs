//! The field-mapping engine (§3, §8, M10).
//!
//! A connection's `field_mapping` JSONB is `{ "fields": { "<canonical>":
//! "<external>" } }`, translating between MES's canonical record fields and
//! whatever field names the customer's ERP uses. Import maps external →
//! canonical; export maps canonical → external. Changing which ERP shape MES
//! talks to is a change to this data, not to any code.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::ErpError;

/// A parsed, direction-agnostic field mapping.
#[derive(Debug, Clone, Default)]
pub struct FieldMapping {
    /// canonical field name → external field name.
    fields: BTreeMap<String, String>,
}

impl FieldMapping {
    /// Parse from a connection's `field_mapping` JSONB. Accepts either
    /// `{ "fields": { ... } }` or a bare `{ ... }` object of canonical→external.
    pub fn from_json(v: &Value) -> Result<Self, ErpError> {
        let obj = match v.get("fields") {
            Some(f) => f,
            None => v,
        };
        let map = obj
            .as_object()
            .ok_or_else(|| ErpError::Mapping("field_mapping must be an object".into()))?;
        let mut fields = BTreeMap::new();
        for (canonical, ext) in map {
            let ext = ext.as_str().ok_or_else(|| {
                ErpError::Mapping(format!("mapping for '{canonical}' must be a string"))
            })?;
            fields.insert(canonical.clone(), ext.to_string());
        }
        Ok(Self { fields })
    }

    /// Import: translate one external record into a canonical record. External
    /// fields with no mapping are dropped; a mapped field absent from the input
    /// is simply omitted.
    pub fn to_canonical(&self, external: &Value) -> Value {
        let mut out = Map::new();
        for (canonical, ext) in &self.fields {
            if let Some(val) = external.get(ext) {
                out.insert(canonical.clone(), val.clone());
            }
        }
        Value::Object(out)
    }

    /// Export: translate one canonical record into the external shape.
    pub fn to_external(&self, canonical: &Value) -> Value {
        let mut out = Map::new();
        for (canon, ext) in &self.fields {
            if let Some(val) = canonical.get(canon) {
                out.insert(ext.clone(), val.clone());
            }
        }
        Value::Object(out)
    }

    /// Number of mapped fields.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn imports_external_to_canonical() {
        let m = FieldMapping::from_json(&json!({
            "fields": { "wo_number": "OrderNo", "part_id": "Item", "qty_ordered": "Qty" }
        }))
        .unwrap();
        let external = json!({ "OrderNo": "WO-1", "Item": "P-1", "Qty": 5, "ignored": true });
        assert_eq!(
            m.to_canonical(&external),
            json!({ "wo_number": "WO-1", "part_id": "P-1", "qty_ordered": 5 })
        );
    }

    #[test]
    fn exports_canonical_to_external() {
        let m = FieldMapping::from_json(&json!({
            "fields": { "code": "sku", "stock": "on_hand" }
        }))
        .unwrap();
        let canonical = json!({ "code": "BRG-1", "stock": 4, "name": "Bearing" });
        assert_eq!(
            m.to_external(&canonical),
            json!({ "sku": "BRG-1", "on_hand": 4 })
        );
    }

    #[test]
    fn a_different_erp_shape_is_only_a_mapping_change() {
        // Same canonical data, two different ERP field vocabularies — no code
        // change, only the mapping differs.
        let canonical = json!({ "code": "BRG-1", "stock": 4 });
        let erp_a =
            FieldMapping::from_json(&json!({"fields": {"code": "sku", "stock": "on_hand"}}))
                .unwrap();
        let erp_b =
            FieldMapping::from_json(&json!({"fields": {"code": "MaterialNo", "stock": "Bestand"}}))
                .unwrap();
        assert_eq!(
            erp_a.to_external(&canonical),
            json!({"sku":"BRG-1","on_hand":4})
        );
        assert_eq!(
            erp_b.to_external(&canonical),
            json!({"MaterialNo":"BRG-1","Bestand":4})
        );
    }

    #[test]
    fn accepts_bare_object_mapping() {
        let m = FieldMapping::from_json(&json!({ "a": "X" })).unwrap();
        assert_eq!(m.to_external(&json!({"a": 1})), json!({"X": 1}));
    }

    #[test]
    fn rejects_non_object() {
        assert!(FieldMapping::from_json(&json!("nope")).is_err());
        assert!(FieldMapping::from_json(&json!({ "fields": { "a": 1 } })).is_err());
    }
}
