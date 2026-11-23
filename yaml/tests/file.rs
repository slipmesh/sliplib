use std::path::Path;

use serde::Deserialize;
use sliplib_yaml::{read, read_or_empty, read_two, replace_file, write_if_changed};

#[test]
fn a_missing_file_reads_as_empty_and_an_existing_one_as_its_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node-a.yaml");
    assert_eq!(read_or_empty(&path).unwrap(), "");
    std::fs::write(&path, "a: 1\n").unwrap();
    assert_eq!(read_or_empty(&path).unwrap(), "a: 1\n");
}

#[test]
fn a_file_that_cannot_be_read_is_an_error_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let err = read_or_empty(dir.path()).unwrap_err();
    assert!(
        format!("{err:#}").contains(&dir.path().display().to_string()),
        "{err:#}"
    );
}

#[test]
fn a_file_is_written_only_when_its_content_changes_and_its_directory_is_created() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("patches").join("node-a.yaml");
    assert!(write_if_changed(&path, "a: 1\n").unwrap());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a: 1\n");
    let written = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert!(!write_if_changed(&path, "a: 1\n").unwrap());
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        written
    );
    assert!(write_if_changed(&path, "a: 2\n").unwrap());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a: 2\n");
}

#[test]
fn a_replaced_file_holds_the_new_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("slipmesh.yaml");
    std::fs::write(&path, "a: 1\n").unwrap();
    replace_file(&path, "a: 2\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a: 2\n");
}

#[cfg(unix)]
#[test]
fn a_replaced_file_keeps_its_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("slipmesh.yaml");
    std::fs::write(&path, "a: 1\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    replace_file(&path, "a: 2\n").unwrap();
    assert_eq!(mode(&path), 0o600);
}

#[cfg(unix)]
#[test]
fn a_file_replaced_through_a_symlink_replaces_its_target() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.yaml");
    let link = dir.path().join("slipmesh.yaml");
    std::fs::write(&target, "a: 1\n").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    replace_file(&link, "a: 2\n").unwrap();
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "a: 2\n");
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[derive(Debug, PartialEq, Deserialize)]
struct Config {
    name: String,
    port: u16,
}

#[test]
fn a_file_reads_as_the_type_it_holds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node-a.yaml");
    std::fs::write(&path, "name: node-a\nport: 179\n").unwrap();
    assert_eq!(
        read::<Config>(&path).unwrap(),
        Config {
            name: "node-a".into(),
            port: 179
        }
    );
}

#[test]
fn a_missing_file_is_an_error_naming_it_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node-a.yaml");
    let err = format!("{:#}", read::<Config>(&path).unwrap_err());
    assert_eq!(err.matches(&path.display().to_string()).count(), 1, "{err}");
}

#[test]
fn a_wrong_value_is_an_error_naming_the_file_once_and_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node-a.yaml");
    std::fs::write(&path, "name: node-a\nport: not-a-port\n").unwrap();
    let err = format!("{:#}", read::<Config>(&path).unwrap_err());
    assert_eq!(err.matches(&path.display().to_string()).count(), 1, "{err}");
    assert!(err.contains("line 2"), "{err}");
}

/// The keys of the file both daemons read.
#[derive(Debug, PartialEq, Deserialize)]
struct Shared {
    interfaces: Vec<Interface>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct Interface {
    name: String,
    listen_port: u16,
}

/// The keys of one daemon alone.
#[derive(Debug, PartialEq, Deserialize)]
struct Own {
    metrics: Metrics,
}

#[derive(Debug, PartialEq, Deserialize)]
struct Metrics {
    port: u16,
}

fn both(listen_port: &str, metrics_port: &str) -> String {
    format!(
        "interfaces:\n  - name: mesh-a\n    listen_port: {listen_port}\nmetrics:\n  port: {metrics_port}\n"
    )
}

fn read_both(text: &str) -> (std::path::PathBuf, anyhow::Result<(Shared, Own)>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("awg.yaml");
    std::fs::write(&path, text).unwrap();
    let read = read_two::<Shared, Own>(&path);
    (path, read)
}

#[test]
fn a_file_of_two_types_reads_as_both() {
    let (_, read) = read_both(&both("51820", "9586"));
    assert_eq!(
        read.unwrap(),
        (
            Shared {
                interfaces: vec![Interface {
                    name: "mesh-a".into(),
                    listen_port: 51820
                }]
            },
            Own {
                metrics: Metrics { port: 9586 }
            }
        )
    );
}

#[test]
fn a_wrong_value_in_the_first_types_keys_names_the_file_its_key_and_line() {
    let (path, read) = read_both(&both("abc", "9586"));
    let err = format!("{:#}", read.unwrap_err());
    assert_eq!(err.matches(&path.display().to_string()).count(), 1, "{err}");
    assert!(err.contains("interfaces[0].listen_port: "), "{err}");
    assert!(err.contains("at line 3 column 18"), "{err}");
}

#[test]
fn a_wrong_value_in_the_second_types_keys_names_the_file_its_key_and_line() {
    let (path, read) = read_both(&both("51820", "abc"));
    let err = format!("{:#}", read.unwrap_err());
    assert_eq!(err.matches(&path.display().to_string()).count(), 1, "{err}");
    assert!(err.contains("metrics.port: "), "{err}");
    assert!(err.contains("at line 5 column 9"), "{err}");
}
