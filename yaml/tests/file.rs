use std::path::Path;

use serde::Deserialize;
use sliplib_yaml::{Error, MultiDoc, read, read_or_empty, replace_file, write_if_changed};

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
        matches!(&err, Error::Reading { path, .. } if path == dir.path()),
        "{err:?}"
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
    let err = chain(read::<Config>(&path).unwrap_err());
    assert_eq!(err.matches(&path.display().to_string()).count(), 1, "{err}");
}

#[derive(Debug, Deserialize)]
struct Interfaces {
    #[expect(dead_code, reason = "read for its refusal")]
    interfaces: Vec<Interface>,
}

#[derive(Debug, Deserialize)]
struct Interface {
    #[expect(dead_code, reason = "read for its refusal")]
    listen_port: u16,
}

#[test]
fn a_wrong_value_is_an_error_naming_the_file_once_its_key_and_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("awg.yaml");
    std::fs::write(
        &path,
        "interfaces:\n  - name: mesh-a\n    listen_port: abc\n",
    )
    .unwrap();
    let err = chain(read::<Interfaces>(&path).unwrap_err());
    assert_eq!(err.matches(&path.display().to_string()).count(), 1, "{err}");
    assert!(
        err.contains("interfaces[0].listen_port: ") && err.contains("at line 3 column 18"),
        "{err}"
    );
}

/// As a caller over `anyhow` prints it.
fn chain(err: Error) -> String {
    format!("{:#}", anyhow::Error::from(err))
}

#[test]
fn a_file_that_is_not_there_is_refused_as_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node-a.yaml");
    for err in [
        read::<Config>(&path).unwrap_err(),
        MultiDoc::read(&path).err().unwrap(),
    ] {
        let Error::Reading { path: read, source } = &err else {
            panic!("{err:?}");
        };
        assert_eq!(read, &path);
        assert!(
            matches!(&**source, Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound),
            "{err:?}"
        );
    }
}

#[test]
fn a_file_that_is_not_yaml_is_refused_as_such_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node-a.yaml");
    std::fs::write(&path, "a: [unterminated\n").unwrap();
    let err = MultiDoc::read(&path).err().unwrap();
    assert!(
        matches!(&err, Error::Reading { path: read, source } if read == &path && matches!(**source, Error::NotYaml(_))),
        "{err:?}"
    );
    assert!(
        matches!(read::<Config>(&path).unwrap_err(), Error::Reading { source, .. } if matches!(*source, Error::Yaml(_)))
    );
}

#[test]
fn a_file_that_is_not_there_is_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("slipmesh.yaml");
    let err = replace_file(&path, "a: 1\n").unwrap_err();
    assert!(
        matches!(&err, Error::Resolving { path: resolved, .. } if resolved == &path),
        "{err:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_directory_that_cannot_be_created_is_refused_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let dangling = dir.path().join("patches");
    std::os::unix::fs::symlink(dir.path().join("nowhere").join("patches"), &dangling).unwrap();
    let err = write_if_changed(&dangling.join("node-a.yaml"), "a: 1\n").unwrap_err();
    assert!(
        matches!(&err, Error::Creating { path, .. } if path == &dangling),
        "{err:?}"
    );
}
