use serde::Deserialize;
use sliplib_yaml::{MultiDoc, Value, append, join};

#[test]
fn documents_keep_their_position_and_line_and_comment_only_ones_are_skipped() {
    let file = MultiDoc::parse("a: 1\n---\n# nothing here yet\n---\n# about b\nb: 2\n").unwrap();
    let found: Vec<(usize, usize)> = file.documents().iter().map(|d| (d.index, d.line)).collect();
    assert_eq!(found, [(0, 1), (2, 6)]);
    assert_eq!(file.documents()[1].value["b"], 2);
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
    let err = format!("{:#}", document.deserialize::<Strict>().unwrap_err());
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
    let err = format!("{:#}", MultiDoc::read(&path).err().unwrap());
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
fn documents_append_after_the_last_line() {
    assert_eq!(append("a: 1", &["b: 2\n".into()]), "a: 1\n---\nb: 2\n");
    assert_eq!(append("a: 1\n", &["b: 2\n".into()]), "a: 1\n---\nb: 2\n");
}
