use serde::Deserialize;
use sliplib_yaml::{MultiDoc, Operation, append, join};

#[test]
fn documents_keep_their_position_and_line_and_comment_only_ones_are_skipped() {
    let file = MultiDoc::parse("a: 1\n---\n# nothing here yet\n---\nb: 2\n").unwrap();
    let documents = file.documents().unwrap();
    let found: Vec<(usize, usize)> = documents.iter().map(|d| (d.index, d.line)).collect();
    assert_eq!(found, [(0, 1), (2, 4)]);
    assert_eq!(documents[1].value["b"], 2);
}

#[test]
fn a_document_ending_in_a_block_scalar_keeps_its_content() {
    let file = MultiDoc::parse("a: 1\nb: |\n  one\n  two\n---\nc: 2\n").unwrap();
    let documents = file.documents().unwrap();
    assert_eq!(documents[0].text(), "a: 1\nb: |\n  one\n  two\n");
    assert_eq!(documents[0].value["b"], "one\ntwo\n");
    assert_eq!(documents[1].text(), "---\nc: 2\n");
}

#[test]
fn a_taken_key_leaves_the_value_and_the_text_with_its_lines_kept() {
    let file = MultiDoc::parse("a: 1\n---\nmeta:\n  kind: x\nfield: 2\n").unwrap();
    let mut document = file.documents().unwrap().remove(1);
    assert_eq!(document.take("meta").unwrap()["kind"], "x");
    assert!(document.value.get("meta").is_none());
    assert!(!document.text().contains("kind"), "{}", document.text());
    assert!(document.text().contains("field: 2"), "{}", document.text());
    assert_eq!(document.text().lines().count(), 4);
    assert_eq!(document.take("absent"), None);
}

#[test]
fn keys_taken_one_after_another_each_leave_only_their_own_lines() {
    let text = "a: 1\n---\nfirst:\n  deep: x\nkept: 1\nsecond: y\nalso_kept: 2\n";
    let mut document = MultiDoc::parse(text)
        .unwrap()
        .documents()
        .unwrap()
        .remove(1);
    document.take("first");
    document.take("second");
    assert_eq!(document.text(), "---\n\n\nkept: 1\n\nalso_kept: 2\n");
}

#[test]
fn a_taken_key_whose_value_is_a_block_scalar_takes_its_content_too() {
    let file = MultiDoc::parse("a: 1\nmeta: |\n  secret line\n").unwrap();
    let mut document = file.documents().unwrap().remove(0);
    assert_eq!(document.take("meta").unwrap(), "secret line\n");
    assert!(
        !document.text().contains("secret line"),
        "{}",
        document.text()
    );
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
#[expect(dead_code)]
struct Strict {
    field: u32,
}

#[test]
fn a_deserialize_error_names_the_field_and_its_line_in_the_file() {
    let file = MultiDoc::parse("a: 1\n---\nmeta: x\nfield: 2\nstray: 3\n").unwrap();
    let mut document = file.documents().unwrap().remove(1);
    document.take("meta");
    let err = format!("{:#}", document.deserialize::<Strict>().unwrap_err());
    assert!(err.contains("stray"), "{err}");
    assert!(err.contains("line 5"), "{err}");
}

fn add(path: &[&str], value: &str) -> Operation {
    Operation::Add {
        path: path.iter().map(|s| s.to_string()).collect(),
        value: yaml_rt::from_str(value).unwrap(),
    }
}

fn remove(path: &[&str]) -> Operation {
    Operation::Remove {
        path: path.iter().map(|s| s.to_string()).collect(),
    }
}

const NETWORK: &str = "\
# hand-written
nodes:
    - name: node-a # first
      node_id: 10.0.0.1
mesh:
    links:
        - pair: [node-a, node-b]
          port: 51820
---
clients:
    - name: phone # mine
      public_key: AAAA
    - name: laptop
      public_key: BBBB
";

#[test]
fn a_field_added_changes_only_its_own_line() {
    let mut file = MultiDoc::parse(NETWORK).unwrap();
    file.patch(0, &[add(&["nodes", "0", "mesh_private_key"], "KEY=")])
        .unwrap();
    assert_eq!(
        file.text(),
        NETWORK.replace(
            "      node_id: 10.0.0.1\n",
            "      node_id: 10.0.0.1\n      mesh_private_key: KEY=\n"
        )
    );
}

#[test]
fn a_mapping_added_is_written_as_a_block() {
    let mut file = MultiDoc::parse(NETWORK).unwrap();
    file.patch(
        0,
        &[add(
            &["mesh", "links", "0", "obfuscation"],
            "{jc: 4, h1: 12345}",
        )],
    )
    .unwrap();
    assert!(
        file.text()
            .contains("          port: 51820\n          obfuscation:\n            jc: 4\n            h1: 12345\n"),
        "{}",
        file.text()
    );
}

#[test]
fn an_item_is_appended_to_a_sequence_and_another_removed_line_and_all() {
    let mut file = MultiDoc::parse(NETWORK).unwrap();
    file.patch(
        1,
        &[
            add(&["clients", "-"], "{name: tablet, public_key: CCCC}"),
            remove(&["clients", "0"]),
        ],
    )
    .unwrap();
    let clients = &file.documents().unwrap()[1].value["clients"];
    let names: Vec<&str> = clients
        .as_sequence()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["laptop", "tablet"]);
    assert!(!file.text().contains("phone"), "{}", file.text());
    assert!(
        file.text()
            .starts_with(NETWORK.split("---").next().unwrap())
    );
}

#[test]
fn an_added_item_indents_its_nested_sequences_as_the_file_does() {
    let text = "clients:\n    - name: phone\n      allowed_ips:\n        - 10.0.0.2/32\n";
    let mut file = MultiDoc::parse(text).unwrap();
    file.patch(
        0,
        &[add(
            &["clients", "-"],
            "{name: tablet, allowed_ips: [10.0.0.3/32]}",
        )],
    )
    .unwrap();
    assert!(
        file.text()
            .ends_with("      allowed_ips:\n        - 10.0.0.3/32\n"),
        "{}",
        file.text()
    );
}

#[test]
fn a_path_segment_with_a_slash_or_tilde_names_one_key() {
    let mut file = MultiDoc::parse("a: 1\n").unwrap();
    file.patch(0, &[add(&["x/y~z"], "1")]).unwrap();
    assert_eq!(file.documents().unwrap()[0].value["x/y~z"], 1);
}

#[test]
fn a_patch_that_fails_changes_nothing_and_names_what_is_missing() {
    let mut file = MultiDoc::parse(NETWORK).unwrap();
    let err = file
        .patch(
            0,
            &[
                add(&["nodes", "0", "mesh_private_key"], "KEY="),
                add(&["missing", "child"], "1"),
            ],
        )
        .unwrap_err();
    assert!(format!("{err:#}").contains("missing"), "{err:#}");
    assert_eq!(file.text(), NETWORK);
}

#[test]
fn patches_one_after_another_each_read_the_file_as_the_last_one_left_it() {
    let mut file = MultiDoc::parse(NETWORK).unwrap();
    file.patch(1, &[remove(&["clients", "0"])]).unwrap();
    file.patch(0, &[add(&["nodes", "0", "mesh_private_key"], "KEY=")])
        .unwrap();
    file.patch(1, &[remove(&["clients", "0"])]).unwrap();
    let documents = file.documents().unwrap();
    assert_eq!(documents[0].value["nodes"][0]["mesh_private_key"], "KEY=");
    assert_eq!(
        documents[1].value["clients"].as_sequence().unwrap().len(),
        0
    );
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
