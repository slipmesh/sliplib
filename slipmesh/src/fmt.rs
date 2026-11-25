//! `fmt`: the file in the form every write leaves it, no value changed, so a later edit shows
//! alone.

use crate::file::{parse_slipmesh, read_text, write_text};
use anyhow::Result;
use sliplib_yaml::MultiDoc;
use std::path::Path;

fn holds_the_values_of(rewritten: &str, original: &MultiDoc) -> Result<bool> {
    let rewritten = MultiDoc::parse(rewritten)?;
    Ok(rewritten.values() == original.values())
}

pub fn run(config_path: &Path) -> Result<()> {
    let original = read_text(config_path)?;
    let (sources, _) = parse_slipmesh(config_path, &original)?;
    let yaml = sources.files()[0].yaml();
    let rewritten = yaml.text()?;
    let path = config_path.display();
    if rewritten == original {
        println!("{path}: already in the form slipmesh writes");
        return Ok(());
    }
    // Its one promise, checked on the text written.
    anyhow::ensure!(
        holds_the_values_of(&rewritten, yaml)?,
        "{path}: rewriting it would change a value - not written"
    );
    write_text(config_path, &rewritten)?;
    println!("rewrote {path} in the form slipmesh writes");
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
