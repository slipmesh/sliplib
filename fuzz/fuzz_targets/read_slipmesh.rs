#![no_main]

use libfuzzer_sys::fuzz_target;
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_mesh::render::node_configs;
use sliplib_mesh::{model, mtu, secrets, warnings};

// Everything a tool does with a file it was given, up to the configs of every node: a text of
// either version may be refused at any step, and none of the steps may panic.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(file) = SlipmeshFile::parse(text) else {
        return;
    };
    if let SlipmeshFile::V2(files) = &file {
        let _ = files.talos_settings();
        let _ = files.routeros_settings();
        let _ = files.missing_reference_values();
        for template in files.templates() {
            let _ = template.read();
        }
        for node in files.network().nodes.keys() {
            let _ = files.templates_for(node.as_str());
            let _ = files.patches_for(node.as_str());
        }
    }
    let Ok(network) = model::lower(&file) else {
        return;
    };
    let resolved = secrets::read(&network);
    let _ = mtu::mesh_mtu(&network, &resolved);
    let _ = warnings::warnings(&network, &resolved);
    for node in &network.nodes {
        let _ = node_configs(&network, &node.name, &resolved);
    }
});
