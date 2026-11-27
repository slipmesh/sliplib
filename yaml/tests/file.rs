use serde::Deserialize;
use sliplib_yaml::{Error, MultiDoc, read};

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
fn a_tag_in_a_file_is_refused_naming_its_key_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node-a.yaml");
    std::fs::write(&path, "name: node-a\nport: !port 179\n").unwrap();
    let err = read::<Config>(&path).unwrap_err();
    assert!(
        matches!(&err, Error::Reading { source, .. } if matches!(&**source, Error::Tagged { key, tag } if key == "port" && tag == "!port")),
        "{err:?}"
    );
}
