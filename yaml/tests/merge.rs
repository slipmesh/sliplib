use sliplib_yaml::{Value, merge};

fn yaml(text: &str) -> Value {
    yaml_serde::from_str(text).unwrap()
}

fn keys(value: &Value) -> Vec<&str> {
    value
        .as_mapping()
        .unwrap()
        .keys()
        .map(|k| k.as_str().unwrap())
        .collect()
}

#[test]
fn mappings_merge_key_by_key_and_anything_else_is_replaced() {
    for (base, overlay, merged) in [
        (
            "extraArgs: {rotate: 'true', level: '2'}",
            "extraArgs: {level: '4'}",
            "extraArgs: {rotate: 'true', level: '4'}",
        ),
        // RFC 7396: a sequence is replaced, not concatenated.
        ("list: [x, y]", "list: [z]", "list: [z]"),
        ("a: 1", "a: {b: 2, c: null}", "a: {b: 2}"),
    ] {
        assert_eq!(
            merge(yaml(base), &yaml(overlay)),
            yaml(merged),
            "{overlay} over {base}"
        );
    }
}

// By the keys: `Mapping` equality ignores their order.
#[test]
fn the_base_key_order_is_kept_with_new_keys_appended_and_a_null_taking_its_key_out() {
    for (base, overlay, order) in [
        (
            "a: 1\nb: 2\nc: 3",
            "new: 0\nb: 20",
            &["a", "b", "c", "new"][..],
        ),
        ("a: 1\nb: 2\nc: 3\nd: 4", "a: null", &["b", "c", "d"]),
    ] {
        let merged = merge(yaml(base), &yaml(overlay));
        assert_eq!(keys(&merged), order, "{overlay:?} over {base:?}");
    }
}

#[test]
fn a_scalar_over_a_mapping_replaces_it() {
    assert_eq!(merge(yaml("a: {b: 2}"), &yaml("a: 1")), yaml("a: 1"));
}
