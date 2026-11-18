//! What a topology does that the tools accept and whoever wrote it should still hear of. Nothing
//! here refuses a file or changes what a node gets.

use crate::mtu::link_name;
use crate::secrets::{ResolvedSecrets, link_key};
use sliplib_config::TrailerLoss;
use sliplib_config::mesh_config::MeshConfig;
use std::fmt;

/// A link that loses data packets it should deliver, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    link: String,
    ends: [String; 2],
    loss: TrailerLoss,
}

impl Warning {
    /// Whether `node` is an end of the link: a tool run for some nodes warns about their links.
    pub fn concerns(&self, node: &str) -> bool {
        self.ends.iter().any(|end| end == node)
    }

    pub fn loss(&self) -> &TrailerLoss {
        &self.loss
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mesh link {}: {}", self.link, self.loss)
    }
}

/// Every warning about the links of `mesh`, in their order, each link's obfuscation taken as
/// `resolved` holds it: its own laid over the topology's, none for a `plain` link.
pub fn warnings(mesh: &MeshConfig, resolved: &ResolvedSecrets) -> Vec<Warning> {
    mesh.mesh
        .links
        .iter()
        .flat_map(|link| {
            resolved
                .mesh_link_obfuscation
                .get(&link_key(&link.pair))
                .map(|obfuscation| obfuscation.trailer_loss())
                .unwrap_or_default()
                .into_iter()
                .map(|loss| Warning {
                    link: link_name(link),
                    ends: link.pair.clone(),
                    loss,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets;

    fn mesh(obfuscation: &str, links: &str) -> MeshConfig {
        sliplib_yaml::from_str(&format!(
            r#"
cluster:
  bgp_as: 64512
  loopback_networks: {{ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}}
obfuscation: {obfuscation}
nodes:
  - {{name: a, node_id: "198.51.100.1", endpoint: "192.0.2.10"}}
  - {{name: b, node_id: "198.51.100.2"}}
  - {{name: c, node_id: "198.51.100.3"}}
mesh:
  links: {links}
"#
        ))
        .unwrap()
    }

    fn told(obfuscation: &str, links: &str) -> Vec<String> {
        let mesh = mesh(obfuscation, links);
        warnings(&mesh, &secrets::read(&mesh))
            .iter()
            .map(Warning::to_string)
            .collect()
    }

    #[test]
    fn a_link_is_warned_about_by_what_the_topology_and_the_link_give_it_together() {
        let told = told(
            "{random_trailers: true}",
            "[{pair: [a, b], port: 51820, obfuscation: {h1: 10-19}}, {pair: [c, a], port: 51821}]",
        );
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(told[0].starts_with("mesh link a-b: "), "{told:?}");
        assert!(told[0].contains("12 packets"), "{told:?}");
    }

    #[test]
    fn a_link_that_turns_the_trailers_off_or_is_plain_is_not_warned_about() {
        let told = told(
            "{random_trailers: true, h1: 10-19, s1: 8}",
            "[{pair: [a, b], port: 51820, obfuscation: {random_trailers: false}}, \
              {pair: [a, c], port: 51821, plain: true}]",
        );
        assert_eq!(told, [] as [String; 0]);
    }

    #[test]
    fn every_loss_of_every_link_is_told_in_the_order_of_the_links() {
        let told = told(
            "{random_trailers: true}",
            "[{pair: [c, a], port: 51820, obfuscation: {s2: 4}}, \
              {pair: [a, b], port: 51821, obfuscation: {h2: 20-29, s3: 15}}]",
        );
        let links: Vec<&str> = told
            .iter()
            .map(|warning| warning.split(": ").next().unwrap())
            .collect();
        assert_eq!(links, ["mesh link c-a", "mesh link a-b", "mesh link a-b"]);
    }

    #[test]
    fn a_warning_concerns_the_two_ends_of_its_link_alone() {
        let mesh = mesh(
            "{random_trailers: true}",
            "[{pair: [a, b], port: 51820, obfuscation: {h1: 10-19}}]",
        );
        let warnings = warnings(&mesh, &secrets::read(&mesh));
        assert!(warnings[0].concerns("a") && warnings[0].concerns("b"));
        assert!(!warnings[0].concerns("c"));
        assert_eq!(warnings[0].loss(), &TrailerLoss::Share { values: 12 });
    }
}
