use yaml_rt::{Mapping, Value};

/// `overlay` merged over `base` as JSON Merge Patch (RFC 7386) does: mappings merge recursively, an
/// explicit `null` deletes the key it names, anything else replaces what was there. Key order is
/// kept - a key `base` has stays where it was, a new one is appended - so the result still reads in
/// the order it was written.
pub fn merge(base: Value, overlay: &Value) -> Value {
    let Some(overlay) = overlay.as_mapping() else {
        return overlay.clone();
    };
    let mut merged = match base {
        Value::Mapping(mapping) => mapping,
        // A mapping over anything else starts from nothing, so its own nulls delete nothing and
        // do not survive as values.
        _ => Mapping::new(),
    };
    for (key, value) in overlay {
        if value.is_null() {
            // `shift_remove`, not `remove`: the latter swaps the last key into the gap.
            merged.shift_remove(key);
            continue;
        }
        let base_value = merged.get(key).cloned().unwrap_or(Value::Null);
        merged.insert(key.clone(), merge(base_value, value));
    }
    Value::Mapping(merged)
}
