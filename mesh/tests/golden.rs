//! Each file of version 2 against the configs version 1 gave the network it was converted from,
//! derived public keys named by their owner.

use serde::{Deserialize, Serialize};
use sliplib_config::awg::{AwgConfig, InterfaceEntry};
use sliplib_config::router::RouterConfig;
use sliplib_mesh::render::node_configs;
use sliplib_yaml::Value;

// Version 1 wrote, beside the shared keys, those only its daemon reads: each type below takes the
// ones of its level alone.

#[derive(Deserialize, Serialize)]
struct Router {
    #[serde(flatten)]
    shared: RouterConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metrics: Option<Value>,
}

#[derive(Deserialize, Serialize)]
struct Awg {
    interfaces: Vec<Interface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metrics: Option<Value>,
}

#[derive(Deserialize, Serialize)]
struct Interface {
    #[serde(flatten)]
    shared: InterfaceEntry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handshake_stale_secs: Option<u64>,
}

impl Awg {
    fn shared(self) -> AwgConfig {
        AwgConfig {
            interfaces: self
                .interfaces
                .into_iter()
                .map(|interface| interface.shared)
                .collect(),
        }
    }
}

/// Version 1's, by directory, with the nodes in the order written.
struct Configs {
    directory: &'static str,
    nodes: &'static [&'static str],
}

const FULL: Configs = Configs {
    directory: "golden/v1",
    nodes: &[
        "node-a", "node-b", "node-c", "node-d", "node-e", "node-f", "router1",
    ],
};

const ALTERNATE: Configs = Configs {
    directory: "golden/v1-alternate",
    nodes: &["east", "west", "south"],
};

/// A network converted by hand, beside version 1's configs.
struct Twin {
    v1: &'static Configs,
    text: &'static str,
    /// What a peer dialed where the node wrote a port into its endpoint, and what it dials now.
    redialed: &'static [(&'static str, &'static str)],
    /// An address of a `node_id` too wide for its network, and the one of its replacement.
    renumbered: &'static [(&'static str, &'static str)],
}

const FULL_TWIN: Twin = Twin {
    v1: &FULL,
    text: include_str!("golden/v2/slipmesh.yaml"),
    redialed: &[("192.0.2.4:51999", "192.0.2.4:51832")],
    renumbered: &[],
};

const ALTERNATE_TWIN: Twin = Twin {
    v1: &ALTERNATE,
    text: include_str!("golden/v2-alternate/slipmesh.yaml"),
    redialed: &[],
    // east was 198.51.100.133 and west 0.0.1.2, cut by version 1 to 0.0.0.5 and 0.0.0.2 in the
    // IPv4 loopback's /25 and not in the IPv6 one's /64.
    renumbered: &[
        ("3fff::1:0:0:c633:6485", "3fff:0:0:1::5"),
        ("fe80::c633:6485", "fe80::5"),
        ("3fff:0:0:1::102", "3fff:0:0:1::2"),
        ("fe80::102", "fe80::2"),
    ],
};

/// Each public key replaced by its owner's name.
fn named(text: String, public_keys: &[(String, String)]) -> String {
    public_keys
        .iter()
        .fold(text, |text, (key, whose)| text.replace(key, whose))
}

fn golden(directory: &str, file: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/{directory}/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// Version 1's configs, less what `docs/slipmesh-yaml-v1-to-v2.md` says a conversion changes and
/// the keys only a daemon's own config holds.
fn the_twin_gives_every_node_what_version_1_gave_it(twin: &Twin) {
    let files = sliplib_config::v2::File::parse(twin.text).unwrap();
    let network = sliplib_mesh::Mesh::of(&files).unwrap();
    let nodes = &network.network().nodes;
    let names: Vec<&str> = nodes.keys().map(|name| name.as_str()).collect();
    assert_eq!(names, twin.v1.nodes);
    let public_keys: Vec<(String, String)> = nodes
        .iter()
        .filter_map(|(name, node)| {
            let key = node.private_key()?.public_key().unwrap();
            Some((key, format!("<public key of {name}>")))
        })
        .collect();
    let named = |text: String| named(text, &public_keys);

    for host in twin.v1.nodes {
        let directory = twin.v1.directory;
        let golden_text = |config: &str| golden(directory, &format!("{host}.{config}.yaml"));
        // Here, not unseen below: a key the types do not take.
        let read_awg: Awg = sliplib_yaml::from_str(&golden_text("awg")).unwrap();
        let read_router: Router = sliplib_yaml::from_str(&golden_text("router")).unwrap();
        let as_read = |config: &str| sliplib_yaml::from_str::<Value>(&golden_text(config)).unwrap();
        assert_eq!(
            sliplib_yaml::to_value(&read_awg).unwrap(),
            as_read("awg"),
            "{directory}/{host}.awg.yaml"
        );
        assert_eq!(
            sliplib_yaml::to_value(&read_router).unwrap(),
            as_read("router"),
            "{directory}/{host}.router.yaml"
        );
        let renumbered = |config: &str| {
            twin.renumbered
                .iter()
                .fold(golden_text(config), |text, (was, now)| {
                    text.replace(was, now)
                })
        };
        let mut expected_awg = sliplib_yaml::from_str::<Awg>(&renumbered("awg"))
            .unwrap()
            .shared();
        let mut expected_router = sliplib_yaml::from_str::<Router>(&renumbered("router"))
            .unwrap()
            .shared;
        expected_awg
            .interfaces
            .retain(|interface| !interface.name.starts_with("rw-"));
        expected_router.bypass = None;
        for announced in &mut expected_router.announce {
            announced.label = None;
        }
        let endpoints = expected_awg
            .interfaces
            .iter_mut()
            .flat_map(|interface| &mut interface.peers)
            .filter_map(|peer| peer.endpoint.as_mut());
        for endpoint in endpoints {
            if let Some((_, now)) = twin.redialed.iter().find(|(was, _)| endpoint == was) {
                *endpoint = (*now).to_owned();
            }
        }
        let got = node_configs(&network, host).unwrap();
        let got_awg: AwgConfig =
            sliplib_yaml::from_str(&named(sliplib_yaml::to_string(&got.awg).unwrap())).unwrap();
        let got_router: RouterConfig =
            sliplib_yaml::from_str(&named(sliplib_yaml::to_string(&got.router).unwrap())).unwrap();
        assert_eq!(
            sliplib_yaml::to_string(&got_awg).unwrap(),
            sliplib_yaml::to_string(&expected_awg).unwrap(),
            "the awg config of {host}"
        );
        assert_eq!(
            sliplib_yaml::to_string(&got_router).unwrap(),
            sliplib_yaml::to_string(&expected_router).unwrap(),
            "the router config of {host}"
        );
    }
}

#[test]
fn the_version_2_twin_of_the_full_file_gives_every_node_what_version_1_gave_it() {
    the_twin_gives_every_node_what_version_1_gave_it(&FULL_TWIN);
}

#[test]
fn the_version_2_twin_of_the_alternate_file_gives_every_node_what_version_1_gave_it() {
    the_twin_gives_every_node_what_version_1_gave_it(&ALTERNATE_TWIN);
}
