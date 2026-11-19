#![no_main]

use libfuzzer_sys::fuzz_target;
use sliplib_config::v2::{Aim, File};

/// What the loader holds of a document's aim: it reaches nodes, each of the document's platform.
fn assert_aimed(files: &File, aim: &Aim) {
    assert!(!aim.nodes().is_empty(), "{aim} reaches no node");
    for node in aim.nodes() {
        let platform = files.network().nodes[node].platform;
        assert_eq!(platform, aim.platform(), "{aim} reaches {node}");
    }
}

/// A node is given every document of `all` that reaches it and no other, the last by rank last.
fn assert_for(node: &str, given: &[&Aim], all: &[&Aim]) {
    assert!(given.iter().all(|aim| aim.reaches(node)), "{node}");
    let reaching = all.iter().filter(|aim| aim.reaches(node)).count();
    assert_eq!(given.len(), reaching, "{node}");
    assert!(given.is_sorted_by_key(|aim| aim.rank()), "{node}");
}

// Everything a tool reads of a file of format version 2: the text may be refused at any step,
// none of the steps may panic, and what is not refused holds what the loader promises of it.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(files) = File::parse(text) else {
        return;
    };
    assert_eq!(files.network().check(), Ok(()));
    let _ = files.talos_settings();
    let _ = files.routeros_settings();
    let templates: Vec<&Aim> = files.templates().iter().map(|t| t.aim()).collect();
    let patches: Vec<&Aim> = files.patches().iter().map(|p| p.aim()).collect();
    for template in files.templates() {
        let _ = template.read();
    }
    for aim in templates.iter().chain(&patches) {
        assert_aimed(&files, aim);
    }
    for node in files.network().nodes.keys() {
        let node = node.as_str();
        let given: Vec<&Aim> = files.templates_for(node).iter().map(|t| t.aim()).collect();
        assert_for(node, &given, &templates);
        let given: Vec<&Aim> = files.patches_for(node).iter().map(|p| p.aim()).collect();
        assert_for(node, &given, &patches);
    }
});
