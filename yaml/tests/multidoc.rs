use serde::Deserialize;
use sliplib_yaml::{Error, MultiDoc, Value, Without, join};

#[test]
fn documents_keep_their_position_and_line_and_comment_only_ones_are_skipped() {
    let file = MultiDoc::parse("a: 1\n---\n# nothing here yet\n---\n# about b\nb: 2\n").unwrap();
    let found: Vec<(usize, usize)> = file.documents().iter().map(|d| (d.index, d.line)).collect();
    assert_eq!(found, [(0, 1), (2, 6)]);
    assert_eq!(file.documents()[1].value["b"], 2);
}

#[test]
fn the_text_read_stays_whatever_is_edited() {
    let text = "# a\na: 1\n";
    let mut file = MultiDoc::parse(text).unwrap();
    file.document_mut(0).unwrap()["a"] = 2.into();
    assert_eq!(file.source(), text);
}

#[test]
fn the_values_of_a_file_are_its_documents_in_order() {
    let file = MultiDoc::parse("a: 1\n---\n# nothing here yet\n---\nb: 2\n").unwrap();
    let expected: Vec<Value> = ["a: 1", "b: 2"]
        .iter()
        .map(|text| sliplib_yaml::from_str(text).unwrap())
        .collect();
    assert_eq!(file.values(), expected);
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct Strict {
    field: u32,
}

#[test]
fn a_taken_key_leaves_the_value_and_is_not_read_as_a_field() {
    let file = MultiDoc::parse("a: 1\n---\nmeta:\n  kind: x\nfield: 2\n").unwrap();
    let mut document = file.documents()[1].clone();
    assert_eq!(document.take("meta").unwrap()["kind"], "x");
    assert!(document.value.get("meta").is_none());
    assert_eq!(document.take("absent"), None);
    assert_eq!(
        document.deserialize::<Strict>().unwrap(),
        Strict { field: 2 }
    );
}

#[test]
fn a_deserialize_error_names_the_field_and_its_line_in_the_file() {
    let file = MultiDoc::parse("a: 1\n---\nmeta: x\nfield: 2\nstray: 3\n").unwrap();
    let mut document = file.documents()[1].clone();
    document.take("meta");
    let err = document.deserialize::<Strict>().unwrap_err();
    assert!(matches!(err, Error::Yaml(_)), "{err:?}");
    let err = err.to_string();
    assert!(err.contains("stray"), "{err}");
    assert!(err.contains("line 5"), "{err}");
}

#[test]
fn an_edited_document_is_read_with_its_edits() {
    let mut file = MultiDoc::parse("a: 1\n---\nfield: 2\n").unwrap();
    file.document_mut(1).unwrap()["field"] = 3.into();
    assert_eq!(file.documents()[1].value["field"], 3);
    assert_eq!(
        file.documents()[1].deserialize::<Strict>().unwrap(),
        Strict { field: 3 }
    );
}

#[test]
fn only_a_document_with_content_can_be_edited() {
    let mut file = MultiDoc::parse("a: 1\n---\n# nothing here yet\n").unwrap();
    assert!(file.document_mut(0).is_some());
    assert!(file.document_mut(1).is_none());
    assert!(file.document_mut(2).is_none());
}

#[test]
fn the_text_holds_every_document_with_content_and_multi_line_strings_as_blocks() {
    let file =
        MultiDoc::parse("a: 1\n---\n# nothing here yet\n---\nscript: |\n  one\n  two\n").unwrap();
    assert_eq!(file.text().unwrap(), "a: 1\n---\nscript: |\n  one\n  two\n");
}

#[test]
fn an_edit_shows_in_the_text_with_the_key_order_kept() {
    let mut file = MultiDoc::parse("b: 1\na: 2\n").unwrap();
    let document = file.document_mut(0).unwrap();
    document["c"] = Value::Sequence(vec!["203.0.113.3/32".into()]);
    document["b"] = 4.into();
    assert_eq!(file.text().unwrap(), "b: 4\na: 2\nc:\n- 203.0.113.3/32\n");
}

#[test]
fn a_file_that_cannot_be_read_is_an_error_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.yaml");
    let err = format!(
        "{:#}",
        anyhow::Error::from(MultiDoc::read(&path).err().unwrap())
    );
    assert!(err.contains("missing.yaml"), "{err}");
}

#[test]
fn documents_join_into_one_text() {
    assert_eq!(
        join(&["a: 1\n".into(), "b: 2\n".into()]),
        "a: 1\n---\nb: 2\n"
    );
}

#[test]
fn a_document_nothing_was_taken_from_reads_whatever_its_keys() {
    let file = MultiDoc::parse("? [a, b]: c\n5: x\nfield: 2\n").unwrap();
    let read: Value = file.documents()[0].deserialize().unwrap();
    assert_eq!(read, file.documents()[0].value);
}

/// `Strict` under `inner`, less its `kind`.
#[derive(Deserialize, Debug)]
struct Outer {
    #[expect(dead_code, reason = "read for its refusal")]
    inner: LessKind,
}

#[derive(Debug)]
struct LessKind(#[expect(dead_code, reason = "read for its refusal")] Strict);

impl<'de> Deserialize<'de> for LessKind {
    fn deserialize<D: serde::Deserializer<'de>>(mapping: D) -> Result<Self, D::Error> {
        mapping.deserialize_map(Without::new(&["kind"])).map(Self)
    }
}

#[test]
fn a_mapping_less_some_keys_refuses_another_by_its_key_and_line_and_a_list() {
    sliplib_yaml::from_str::<Outer>("inner:\n  kind: x\n  field: 2\n").unwrap();
    let stray = sliplib_yaml::from_str::<Outer>("inner:\n  kind: x\n  stray: 3\n")
        .unwrap_err()
        .to_string();
    assert!(
        stray.starts_with("inner: unknown field `stray`") && stray.contains("at line 3 column 3"),
        "{stray}"
    );
    let list = sliplib_yaml::from_str::<Outer>("inner: [2]\n")
        .unwrap_err()
        .to_string();
    assert!(list.contains("expected a mapping"), "{list}");
}

#[test]
fn a_tag_is_refused_naming_its_document_and_key_path() {
    for (text, line, key, tag) in [
        (
            "a: 1\n---\nslipmesh:\n  kind: !network\n",
            3,
            "slipmesh.kind",
            "!network",
        ),
        ("a: 1\n---\nb:\n  c: !x 1\n", 3, "b.c", "!x"),
        ("a: [1, !x 2]\n", 1, "a[1]", "!x"),
    ] {
        let err = MultiDoc::parse(text).err().unwrap();
        let Error::InDocument { line: at, source } = &err else {
            panic!("{text}: {err:?}");
        };
        assert_eq!(*at, line, "{text}");
        assert!(
            matches!(&**source, Error::Tagged { key: k, tag: t } if k == key && t == tag),
            "{text}: {err:?}"
        );
    }
}

#[test]
fn a_core_tag_reads_as_the_plain_value() {
    let file = MultiDoc::parse("a: !!str 5\n").unwrap();
    assert_eq!(file.documents()[0].value["a"], "5");
}
