use crate::common::{Config, MESH, edited, mint, minted};

#[cfg(unix)]
#[test]
fn mint_through_a_symlink_writes_into_its_target_and_keeps_the_link() {
    let config = Config::holding(MESH);
    let target = config.path.with_file_name("target.yaml");
    std::fs::rename(&config.path, &target).unwrap();
    std::os::unix::fs::symlink(&target, &config.path).unwrap();
    mint(&config);
    assert!(
        std::fs::symlink_metadata(&config.path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        std::fs::read_to_string(&target)
            .unwrap()
            .contains("private_key")
    );
}

#[test]
fn mint_with_no_secret_to_create_still_writes_the_reference_value_the_file_leaves_out() {
    let config = minted();
    edited(&config, |network| {
        network["mesh"]
            .as_mapping_mut()
            .unwrap()
            .shift_remove("interface_prefix");
    });
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert!(!ran.stdout.contains("minted"), "{}", ran.stdout);
    assert_eq!(config.documents()[0]["mesh"]["interface_prefix"], "mesh-");
}

#[test]
fn mint_writes_nothing_and_names_the_link_whose_headers_leave_no_room() {
    let config = Config::holding(MESH);
    edited(&config, |network| {
        network["mesh"]["links"]["a-b"]["amneziawg"]["obfuscation"] =
            sliplib_yaml::from_str("{h1: 5-2147483647}").unwrap();
    });
    let before = config.text();
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(1), "{}", ran.stderr);
    assert!(
        ran.stderr.contains("mesh.links.a-b.amneziawg.obfuscation"),
        "{}",
        ran.stderr
    );
    assert_eq!(ran.stdout, "");
    assert_eq!(config.text(), before);
}

#[test]
fn mint_writes_nothing_when_a_node_of_version_2_would_be_refused_with_what_it_minted() {
    // No `jmax` the generator draws reaches this `jmin`, so the link's obfuscation is one the
    // kernel module refuses whatever is minted beside it.
    let config = Config::holding(MESH);
    edited(&config, |network| {
        network["mesh"]["links"]["a-b"]["amneziawg"]["obfuscation"] =
            sliplib_yaml::from_str("{jmin: 1100}").unwrap();
    });
    let before = config.text();
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(1));
    assert_eq!(ran.stdout, "");
    assert!(
        ran.stderr.contains("interface \"mesh-node-b\""),
        "{}",
        ran.stderr
    );
    assert_eq!(config.text(), before);
}

#[test]
fn files_the_loader_refuses_are_left_as_they_are_by_mint_and_rekey() {
    let content = MESH.replace(
        "  interface_prefix: mesh-\n",
        "  interface_prefix: mesh-\n  no_such_setting: 1\n",
    );
    assert_ne!(content, MESH);
    for command in [&["mint"][..], &["rekey", "--node", "node-a"][..]] {
        let config = Config::holding(&content);
        let ran = config.run(command);
        assert_eq!(ran.code, Some(1), "{command:?}");
        assert_eq!(ran.stdout, "", "{command:?}");
        assert!(ran.stderr.contains("no_such_setting"), "{}", ran.stderr);
        assert_eq!(config.text(), content, "{command:?}");
    }
}

#[test]
fn mint_with_nothing_to_write_leaves_files_of_version_2_and_their_comments_as_they_are() {
    let config = minted();
    let commented = format!("# kept by hand\n{}", config.text());
    std::fs::write(&config.path, &commented).unwrap();
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(ran.stdout, "");
    assert_eq!(config.text(), commented);
}
