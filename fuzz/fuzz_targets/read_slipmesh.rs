#![no_main]

use libfuzzer_sys::fuzz_target;
use sliplib_config::v2::File;

// Everything a tool reads of a file of format version 2: the text may be refused at any step,
// and none of the steps may panic.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(files) = File::parse(text) else {
        return;
    };
    let _ = files.talos_settings();
    let _ = files.routeros_settings();
    for template in files.templates() {
        let _ = template.read();
    }
    for node in files.network().nodes.keys() {
        let _ = files.templates_for(node.as_str());
        let _ = files.patches_for(node.as_str());
    }
});
