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
fn the_later_scalar_wins() {
    assert_eq!(merge(yaml("a: 1"), &yaml("a: 2")), yaml("a: 2"));
}

#[test]
fn mappings_merge_recursively() {
    let merged = merge(
        yaml("extraArgs: {rotate: 'true', level: '2'}"),
        &yaml("extraArgs: {level: '4'}"),
    );
    assert_eq!(merged, yaml("extraArgs: {rotate: 'true', level: '4'}"));
}

#[test]
fn the_base_key_order_is_kept_and_new_keys_are_appended() {
    let merged = merge(yaml("a: 1\nb: 2\nc: 3"), &yaml("new: 0\nb: 20"));
    assert_eq!(keys(&merged), ["a", "b", "c", "new"]);
}

#[test]
fn a_sequence_is_replaced_not_concatenated() {
    assert_eq!(
        merge(yaml("list: [x, y]"), &yaml("list: [z]")),
        yaml("list: [z]")
    );
}

#[test]
fn an_explicit_null_deletes_the_key_and_keeps_the_rest_in_order() {
    let merged = merge(yaml("a: 1\nb: 2\nc: 3\nd: 4"), &yaml("a: null"));
    assert_eq!(keys(&merged), ["b", "c", "d"]);
}

#[test]
fn a_null_for_a_key_the_base_lacks_adds_nothing() {
    assert_eq!(merge(yaml("a: 1"), &yaml("b: null")), yaml("a: 1"));
}

#[test]
fn a_key_absent_from_the_overlay_is_inherited() {
    assert_eq!(
        merge(yaml("a: 1\nb: {c: 2}"), &yaml("a: 5")),
        yaml("a: 5\nb: {c: 2}")
    );
}

#[test]
fn a_mapping_over_a_scalar_replaces_it_without_its_nulls() {
    assert_eq!(
        merge(yaml("a: 1"), &yaml("a: {b: 2, c: null}")),
        yaml("a: {b: 2}")
    );
}

#[test]
fn a_scalar_over_a_mapping_replaces_it() {
    assert_eq!(merge(yaml("a: {b: 2}"), &yaml("a: 1")), yaml("a: 1"));
}
