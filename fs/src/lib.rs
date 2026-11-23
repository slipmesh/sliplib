//! Files written whole or not at all: a reader sees the old text or the new one.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Each names the path and the step.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("reading {}", path.display())]
    Reading {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("creating {}", path.display())]
    Creating {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("resolving {}", path.display())]
    Resolving {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("opening {} for writing", path.display())]
    Opening {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("writing {}", path.display())]
    Writing {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Empty where there is no file; any other failure is an error.
pub fn read_or_empty(path: &Path) -> Result<String, Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(Error::Reading {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Unless the file already holds it, creating the directory. Whether it wrote.
pub fn write_if_changed(path: &Path, content: &str) -> Result<bool, Error> {
    if read_or_empty(path)? == content && path.exists() {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|source| Error::Creating {
            path: dir.to_owned(),
            source,
        })?;
    }
    write_atomically(path, content)?;
    Ok(true)
}

/// Keeping its permissions and following a symlink.
pub fn replace_file(path: &Path, content: &str) -> Result<(), Error> {
    let target = std::fs::canonicalize(path).map_err(|source| Error::Resolving {
        path: path.to_owned(),
        source,
    })?;
    write_atomically(&target, content)
}

fn write_atomically(path: &Path, content: &str) -> Result<(), Error> {
    let mut file =
        atomic_write_file::AtomicWriteFile::open(path).map_err(|source| Error::Opening {
            path: path.to_owned(),
            source,
        })?;
    file.write_all(content.as_bytes())
        .and_then(|()| file.commit())
        .map_err(|source| Error::Writing {
            path: path.to_owned(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
    fn an_empty_text_is_written_where_there_is_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("node-a.yaml");
        assert!(write_if_changed(&path, "").unwrap());
        assert!(path.exists());
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
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
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
}
