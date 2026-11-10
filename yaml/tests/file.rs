use std::path::Path;

use serde::Deserialize;
use sliplib_yaml::{read, read_or_empty, replace_file, write_if_changed};

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
