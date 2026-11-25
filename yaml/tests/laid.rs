use serde::Deserialize;
use sliplib_yaml::{MultiDoc, Step, Value, from_value_at, merge_keeping, written_by};

fn yaml(text: &str) -> Value {
    sliplib_yaml::from_str(text).unwrap()
}

fn key(name: &str) -> Step {
    Step::Key(name.to_owned())
}

/// Below `a`.
fn under_a(path: &[&Value]) -> bool {
    path.len() > 1 && *path[0] == "a"
}

#[test]
fn a_null_where_it_is_kept_is_laid_as_a_value_and_elsewhere_takes_its_key_out() {
    let base = yaml("{a: {x: 1, y: 2}, b: {x: 1, y: 2}}");
    let laid = merge_keeping(base, &yaml("{a: {x: null}, b: {x: null}}"), &under_a);
    assert_eq!(laid, yaml("{a: {x: null, y: 2}, b: {y: 2}}"));
    let first = merge_keeping(Value::Null, &yaml("{a: {x: null}, b: {x: null}}"), &under_a);
    assert_eq!(first, yaml("{a: {x: null}, b: {}}"));
    let block = merge_keeping(yaml("{a: {x: null}}"), &yaml("{a: null}"), &under_a);
    assert_eq!(block, yaml("{}"));
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
#[expect(dead_code, reason = "read for its refusals")]
struct Tree {
    nodes: std::collections::BTreeMap<String, Node>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
#[expect(dead_code, reason = "read for its refusals")]
struct Node {
    ports: Vec<u16>,
}

#[test]
fn a_value_read_whole_is_refused_by_the_path_of_its_key() {
    for (tree, path) in [
        (
            "{nodes: {a: {ports: [1], colour: red}}}",
            vec![key("nodes"), key("a"), key("colour")],
        ),
        (
            "{nodes: {a: {ports: [1, x]}}}",
            vec![key("nodes"), key("a"), key("ports"), Step::Index(1)],
        ),
        ("{nodes: {a: {}}}", vec![key("nodes"), key("a")]),
    ] {
        let refused = from_value_at::<Tree>(yaml(tree)).unwrap_err();
        assert_eq!(refused.path(), path, "{tree}");
    }
}

#[test]
fn a_path_is_written_by_the_last_layer_holding_it_or_its_deepest_part() {
    let layers = [
        yaml("{nodes: {a: {ports: [1]}, b: {ports: [2]}}}"),
        yaml("{nodes: {a: {ports: [3]}}}"),
        yaml("{nodes: {a: {ports: null}}}"),
    ];
    let layers: Vec<&Value> = layers.iter().collect();
    let ports = [key("nodes"), key("a"), key("ports"), Step::Index(0)];
    assert_eq!(written_by(&layers, &ports), Some((1, 4)));
    let missing = [key("nodes"), key("b"), key("colour")];
    assert_eq!(written_by(&layers, &missing), Some((0, 2)));
    assert_eq!(written_by(&layers, &[]), Some((2, 0)));
    assert_eq!(written_by(&[], &ports), None);
}

#[test]
fn a_key_named_like_a_number_or_a_boolean_is_held_under_its_value() {
    let layers = [yaml("{7: {ports: [1]}, true: {ports: [2]}}")];
    let layers: Vec<&Value> = layers.iter().collect();
    assert_eq!(written_by(&layers, &[key("7"), key("ports")]), Some((0, 2)));
    assert_eq!(
        written_by(&layers, &[key("true"), key("ports")]),
        Some((0, 2))
    );
    assert_eq!(written_by(&layers, &[key("false")]), Some((0, 0)));
}

#[test]
fn the_line_of_a_path_is_that_of_its_last_key_or_item() {
    let text = "a: 1\n---\nnodes:\n  b: {}\n  a:\n    ports:\n    - 1\n    - 2\n  7: {ports: [3]}\n  -1: {}\n  true: {}\n";
    let file = MultiDoc::parse(text).unwrap();
    let document = &file.documents()[1];
    for (path, line) in [
        (vec![key("nodes")], Some(3)),
        (vec![key("nodes"), key("a"), key("ports")], Some(6)),
        (
            vec![key("nodes"), key("a"), key("ports"), Step::Index(1)],
            Some(8),
        ),
        (vec![key("nodes"), key("7"), key("ports")], Some(9)),
        (vec![key("nodes"), key("-1")], Some(10)),
        (vec![key("nodes"), key("true")], Some(11)),
        (vec![key("nodes"), key("c")], None),
        (
            vec![key("nodes"), key("a"), key("ports"), Step::Index(2)],
            None,
        ),
    ] {
        assert_eq!(document.line_at(&path), line, "{path:?}");
    }
}

#[test]
fn an_edited_document_has_no_line_for_a_path() {
    let mut file = MultiDoc::parse("nodes: {a: 1}\n").unwrap();
    file.document_mut(0).unwrap()["nodes"]["a"] = 2.into();
    assert_eq!(file.documents()[0].line_at(&[key("nodes")]), None);
}
