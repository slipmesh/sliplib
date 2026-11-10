use sliplib_config::talos::{
    ConfigFile, ExtensionServiceConfig, Identity, contents_as_text, sets_file,
};
use sliplib_yaml::{Value, from_str, to_value};

fn documents(text: &str) -> Vec<Value> {
    sliplib_yaml::MultiDoc::parse(text)
        .unwrap()
        .documents()
        .iter()
        .map(|d| d.value.clone())
        .collect()
}

const PATCH: &str = "\
apiVersion: v1alpha1
kind: ExtensionServiceConfig
name: awg
configFiles:
  - mountPath: /etc/node-a/awg.yaml
    content: |
      interfaces: []
  - mountPath: /etc/node-a/extra.conf
    content: extra
---
apiVersion: v1alpha1
kind: OtherConfig
name: router
---
apiVersion: v1alpha1
kind: ExtensionServiceConfig
name: router
configFiles:
  - mountPath: /etc/node-a/router.yaml
    content: |
      bgp_as: 64512
";

#[test]
fn a_documents_identity_is_its_api_version_kind_and_name() {
    let documents = documents(PATCH);
    assert_eq!(
        Identity::of(&documents[0]),
        Identity {
            api_version: Some("v1alpha1".into()),
            kind: Some("ExtensionServiceConfig".into()),
            name: Some("awg".into()),
        }
    );
    let bare: Value = from_str("kind: HostnameConfig\n").unwrap();
    assert_eq!(Identity::of(&bare).api_version, None);
    assert_ne!(Identity::of(&documents[1]), Identity::of(&documents[2]));
}

#[test]
fn an_identity_reads_as_its_kind_and_name() {
    let documents = documents(PATCH);
    assert_eq!(
        Identity::of(&documents[0]).to_string(),
        "ExtensionServiceConfig/awg"
    );
    let unnamed = from_str("kind: HostnameConfig\n").unwrap();
    assert_eq!(Identity::of(&unnamed).to_string(), "HostnameConfig");
}

#[test]
fn an_extension_service_config_is_written_as_talos_reads_it() {
    let document = ExtensionServiceConfig::new(
        "awg",
        vec![ConfigFile {
            mount_path: "/etc/node-a/awg.yaml".into(),
            content: "interfaces: []\n".into(),
        }],
    );
    let expected: Value = from_str(
        "apiVersion: v1alpha1\nkind: ExtensionServiceConfig\nname: awg\nconfigFiles:\n  - \
         mountPath: /etc/node-a/awg.yaml\n    content: \"interfaces: []\\n\"\n",
    )
    .unwrap();
    assert_eq!(to_value(&document).unwrap(), expected);
}

#[test]
fn an_extension_service_config_is_found_by_its_name_and_kind() {
    let documents = documents(PATCH);
    let router = ExtensionServiceConfig::find(&documents, "router")
        .unwrap()
        .unwrap();
    assert_eq!(router.config_files[0].content, "bgp_as: 64512\n");
    let awg = ExtensionServiceConfig::find(&documents, "awg")
        .unwrap()
        .unwrap();
    assert_eq!(awg.file("/etc/node-a/extra.conf").unwrap().content, "extra");
    assert_eq!(awg.file("/etc/node-a/missing.conf"), None);
    assert_eq!(
        ExtensionServiceConfig::find(&documents, "nftables").unwrap(),
        None
    );
}

#[test]
fn a_malformed_extension_service_config_is_an_error_naming_it() {
    let documents = documents("kind: ExtensionServiceConfig\nname: awg\nconfigFiles: 1\n");
    let err = format!(
        "{:#}",
        ExtensionServiceConfig::find(&documents, "awg").unwrap_err()
    );
    assert!(err.contains("\"awg\""), "{err}");
}

#[test]
fn a_document_sets_a_file_of_its_own_service_only() {
    let documents = documents(PATCH);
    assert!(sets_file(&documents[0], "awg", "/etc/node-a/extra.conf"));
    assert!(!sets_file(&documents[0], "awg", "/etc/node-a/router.yaml"));
    assert!(!sets_file(
        &documents[0],
        "router",
        "/etc/node-a/extra.conf"
    ));
    assert!(!sets_file(
        &documents[1],
        "router",
        "/etc/node-a/router.yaml"
    ));
}

#[test]
fn a_content_written_as_yaml_becomes_its_text() {
    let mut document: Value = from_str(
        "kind: ExtensionServiceConfig\nconfigFiles:\n  - content:\n      a: 1\n  - content:\n      - \
         b\n  - content: plain\n",
    )
    .unwrap();
    contents_as_text(&mut document).unwrap();
    let contents: Vec<&str> = document["configFiles"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|file| file["content"].as_str().unwrap())
        .collect();
    assert_eq!(contents, ["a: 1\n", "- b\n", "plain"]);
    let mut no_files: Value = from_str("kind: HostnameConfig\n").unwrap();
    contents_as_text(&mut no_files).unwrap();
    assert_eq!(
        no_files,
        from_str::<Value>("kind: HostnameConfig\n").unwrap()
    );
}
