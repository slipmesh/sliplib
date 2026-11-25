use yaml_serde::{Mapping, Value};

/// JSON Merge Patch (RFC 7396), key order kept: a key `base` has stays, a new one is appended.
pub fn merge(base: Value, overlay: &Value) -> Value {
    merge_keeping(base, overlay, &|_| false)
}

/// As [`merge`], but a `null` at a key whose path from the top `keeps` takes is laid as a value.
pub fn merge_keeping(base: Value, overlay: &Value, keeps: &dyn Fn(&[&Value]) -> bool) -> Value {
    lay(base, overlay, &mut Vec::new(), keeps)
}

fn lay<'a>(
    base: Value,
    overlay: &'a Value,
    path: &mut Vec<&'a Value>,
    keeps: &dyn Fn(&[&Value]) -> bool,
) -> Value {
    let Some(overlay) = overlay.as_mapping() else {
        return overlay.clone();
    };
    let mut merged = match base {
        Value::Mapping(mapping) => mapping,
        // Over anything else a mapping starts empty, with nothing for a `null` to remove.
        _ => Mapping::new(),
    };
    for (key, value) in overlay {
        path.push(key);
        if value.is_null() && !keeps(path) {
            // `remove` would swap the last key into the gap.
            merged.shift_remove(key);
        } else {
            let base_value = merged.get(key).cloned().unwrap_or(Value::Null);
            merged.insert(key.clone(), lay(base_value, value, path, keeps));
        }
        path.pop();
    }
    Value::Mapping(merged)
}
