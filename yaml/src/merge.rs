use yaml_serde::{Mapping, Value};

/// JSON Merge Patch (RFC 7386), key order kept: a key `base` has stays, a new one is appended.
pub fn merge(base: Value, overlay: &Value) -> Value {
    let Some(overlay) = overlay.as_mapping() else {
        return overlay.clone();
    };
    let mut merged = match base {
        Value::Mapping(mapping) => mapping,
        // Over anything else a mapping starts empty, so its nulls neither delete nor stay.
        _ => Mapping::new(),
    };
    for (key, value) in overlay {
        if value.is_null() {
            // `remove` would swap the last key into the gap.
            merged.shift_remove(key);
            continue;
        }
        let base_value = merged.get(key).cloned().unwrap_or(Value::Null);
        merged.insert(key.clone(), merge(base_value, value));
    }
    Value::Mapping(merged)
}
