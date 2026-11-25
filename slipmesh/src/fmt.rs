//! `fmt`: each file in the form every write leaves it, no value changed, so a later edit shows
//! alone.

use anyhow::{Context, Result};
use sliplib_config::v2::{Sources, check_blocks};
use sliplib_yaml::MultiDoc;
use std::path::PathBuf;

fn holds_the_values_of(rewritten: &str, original: &MultiDoc) -> Result<bool> {
    let rewritten = MultiDoc::parse(rewritten)?;
    Ok(rewritten.values() == original.values())
}

/// Each file alone, laying nothing: none is written unless every one may be.
pub fn run(paths: &[PathBuf]) -> Result<()> {
    let mut rewrites = Vec::new();
    for path in paths {
        let original =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let sources = Sources::read(&[path], None)?;
        check_blocks(&sources)?;
        let yaml = sources.files()[0].yaml();
        let rewritten = yaml.text()?;
        // Its one promise, checked on the text written.
        anyhow::ensure!(
            holds_the_values_of(&rewritten, yaml)?,
            "{}: rewriting it would change a value - not written",
            path.display()
        );
        rewrites.push((path, rewritten == original, rewritten));
    }
    for (path, unchanged, rewritten) in rewrites {
        if unchanged {
            println!("{}: already in the form slipmesh writes", path.display());
        } else {
            sliplib_fs::replace_file(path, &rewritten)?;
            println!("rewrote {} in the form slipmesh writes", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: &str = "a: {b: 1}\n---\nc: [2, 3]\n";

    fn holds(rewritten: &str) -> bool {
        holds_the_values_of(rewritten, &MultiDoc::parse(ORIGINAL).unwrap()).unwrap()
    }

    #[test]
    fn the_same_values_in_another_layout_are_held() {
        assert!(holds("a:\n  b: 1\n---\nc:\n- 2\n- 3\n"));
    }

    #[test]
    fn a_changed_value_a_lost_document_or_documents_in_another_order_are_not_held() {
        assert!(!holds("a:\n  b: 2\n---\nc:\n- 2\n- 3\n"));
        assert!(!holds("a:\n  b: 1\n"));
        assert!(!holds("c:\n- 2\n- 3\n---\na:\n  b: 1\n"));
    }
}
