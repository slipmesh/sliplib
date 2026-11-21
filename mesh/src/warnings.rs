//! What a topology does that the tools accept and whoever wrote it should still hear of. Nothing
//! here refuses a file or changes what a node gets.

use crate::model::Network;
use crate::secrets::{ResolvedSecrets, link_key};
use sliplib_config::TrailerLoss;
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
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mesh link {}: {}", self.link, self.loss)
    }
}

/// Every warning about the links of `mesh`, in their order, each link's obfuscation taken as
/// `resolved` holds it: its own laid over the network's, none for a `plain` link.
pub fn warnings(mesh: &Network, resolved: &ResolvedSecrets) -> Vec<Warning> {
    mesh.links
        .iter()
        .flat_map(|link| {
            resolved
                .mesh_link_obfuscation
                .get(&link_key(&link.ends))
                .map(|obfuscation| obfuscation.trailer_loss())
                .unwrap_or_default()
                .into_iter()
                .map(|loss| Warning {
                    link: link.name.clone(),
                    ends: link.ends.clone(),
                    loss,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The links of the example of the specification with its secrets, and `overlay` laid over
    /// both: what is told of them.
    fn told_of(overlay: &str) -> Vec<Warning> {
        let network = crate::test_support::example(overlay);
        warnings(&network, &crate::secrets::read(&network))
    }

    fn lines(warnings: &[Warning]) -> Vec<String> {
        warnings.iter().map(Warning::to_string).collect()
    }

    #[test]
    fn the_example_loses_nothing() {
        assert_eq!(told_of("{}"), []);
    }

    #[test]
    fn a_link_is_warned_about_by_what_its_type_and_the_link_give_it_together() {
        // The type turns the trailers on; the link's headers lie in one range of ten.
        let told = lines(&told_of(
            "{mesh: {links: {a-b: {amneziawg: {obfuscation: {h1: 10-19}}}}}}",
        ));
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(told[0].starts_with("mesh link a-b: "), "{told:?}");
        assert!(told[0].contains("12 packets"), "{told:?}");
        let off = told_of(
            "{mesh: {links: {a-b: {amneziawg: \
             {obfuscation: {h1: 10-19, random_trailers: false}}}}}}",
        );
        assert_eq!(off, []);
    }

    #[test]
    fn every_loss_of_each_named_link_is_told_in_the_order_written() {
        // Without its header protection key, an `s1` or `s2` 4 bytes above the type's `s4` of 16
        // reads a data packet's header. `1-b-c`, written after the example's links, sorts before
        // them.
        let told = lines(&told_of(
            "{mesh: {links: {a-b: {amneziawg: {obfuscation: {h2: 20-29, s1: 20, \
             header_protection_key: null}}}, a-c: {amneziawg: {obfuscation: {s2: 20}}}, \
             1-b-c: {type: amneziawg, ends: {node-b: {}, node-c: {}}, amneziawg: {port: 51833, \
             obfuscation: {jc: 4, jmin: 40, jmax: 400, s1: 64, s2: 20, h1: 1101, h2: 2202, \
             h3: 3303, h4: 4404}}}}}}",
        ));
        let links: Vec<&str> = told
            .iter()
            .map(|warning| warning.split(": ").next().unwrap())
            .collect();
        assert_eq!(
            links,
            [
                "mesh link a-b",
                "mesh link a-b",
                "mesh link a-c",
                "mesh link 1-b-c"
            ]
        );
    }

    #[test]
    fn a_warning_of_a_link_concerns_its_two_ends_alone() {
        let told = told_of("{mesh: {links: {a-b: {amneziawg: {obfuscation: {h1: 10-19}}}}}}");
        assert!(told[0].concerns("node-a") && told[0].concerns("node-b"));
        assert!(!told[0].concerns("node-c"));
    }
}
