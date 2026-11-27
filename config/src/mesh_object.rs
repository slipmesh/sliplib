//! The one object in a cluster that tells its agents every node's endpoint, the cluster-scoped
//! `Mesh` called `mesh`. `slipmesh manifest` writes it; with the `kube` feature this is also the
//! custom resource the agents read and the CRD it is checked by.

use crate::v2::network::{Endpoint, Network};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const API_VERSION: &str = "slipmesh.net/v1alpha1";
pub const KIND: &str = "Mesh";
pub const NAME: &str = "mesh";

/// Read strictly: a new field reaches the agents before the CRD and the manifest carry it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "kube", derive(kube::CustomResource, schemars::JsonSchema))]
#[cfg_attr(
    feature = "kube",
    kube(group = "slipmesh.net", version = "v1alpha1", kind = "Mesh")
)]
#[serde(deny_unknown_fields)]
pub struct MeshSpec {
    /// Every node of the network, by its name there.
    pub nodes: BTreeMap<String, MeshNode>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "kube", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MeshNode {
    /// As the files write it; none behind NAT.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<Endpoint>,
}

impl MeshSpec {
    pub fn of(network: &Network) -> Self {
        let nodes = network
            .nodes
            .iter()
            .map(|(name, node)| {
                let endpoint = node
                    .underlay()
                    .and_then(|underlay| underlay.endpoint.clone());
                (name.as_str().to_owned(), MeshNode { endpoint })
            })
            .collect();
        Self { nodes }
    }
}

/// The `Mesh` object as a manifest, without the kube types.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest<'a> {
    api_version: &'static str,
    kind: &'static str,
    metadata: Metadata,
    spec: &'a MeshSpec,
}

#[derive(Serialize)]
struct Metadata {
    name: &'static str,
}

pub fn manifest(spec: &MeshSpec) -> Manifest<'_> {
    Manifest {
        api_version: API_VERSION,
        kind: KIND,
        metadata: Metadata { name: NAME },
        spec,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::File;

    const EXAMPLE: &str = include_str!("../../docs/example/mesh.yaml");

    fn spec(yaml: &str) -> MeshSpec {
        sliplib_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn a_mesh_has_every_node_with_its_endpoint_as_the_files_write_it() {
        let file = File::parse(EXAMPLE).unwrap();
        assert_eq!(
            MeshSpec::of(file.network()),
            spec(
                "nodes:
                   node-a: {endpoint: {ipv4: 192.0.2.1}}
                   node-b: {endpoint: {ipv4: node-b.example.com}}
                   node-c: {}
                   router1: {endpoint: {ipv4: 192.0.2.4}}"
            )
        );
    }

    #[test]
    fn a_mesh_reads_back_as_it_was_written() {
        let written = spec(
            "nodes: {node-a: {endpoint: {ipv6: 2001:db8::1}}, node-b: {endpoint: {ipv4: \
             node-b.example.com}}, node-c: {}}",
        );
        let text = sliplib_yaml::to_string(&manifest(&written)).unwrap();
        let read: sliplib_yaml::Value = sliplib_yaml::from_str(&text).unwrap();
        assert_eq!(read["apiVersion"], API_VERSION);
        assert_eq!(read["kind"], KIND);
        assert_eq!(read["metadata"]["name"], NAME);
        assert_eq!(
            sliplib_yaml::from_value::<MeshSpec>(read["spec"].clone()).unwrap(),
            written
        );
    }

    #[cfg(feature = "kube")]
    #[test]
    fn the_manifest_is_the_object_kube_writes() {
        let spec = spec("nodes: {node-a: {endpoint: {ipv4: 192.0.2.1}}, node-c: {}}");
        assert_eq!(
            serde_json::to_value(manifest(&spec)).unwrap(),
            serde_json::to_value(Mesh::new(NAME, spec)).unwrap()
        );
    }

    #[cfg(feature = "kube")]
    #[test]
    fn the_crd_refuses_an_endpoint_of_both_families_of_none_or_of_null() {
        use crate::v2::network::EndpointKeys;
        use kube::CustomResourceExt;
        let crd = serde_json::to_value(Mesh::crd()).unwrap();
        let node = "/spec/versions/0/schema/openAPIV3Schema/properties/spec/properties/nodes/additionalProperties";
        let endpoint = crd.pointer(&format!("{node}/properties/endpoint")).unwrap();
        assert!(
            endpoint.get("x-kubernetes-validations").is_some(),
            "{endpoint}"
        );
        for family in ["ipv4", "ipv6"] {
            let field = &endpoint["properties"][family];
            assert_eq!(field["type"], "string", "{endpoint}");
            assert!(field.get("nullable").is_none(), "{endpoint}");
        }
        for (endpoint, valid) in [
            (serde_json::json!({"ipv4": "192.0.2.1"}), true),
            (serde_json::json!({"ipv6": "2001:db8::1"}), true),
            (
                serde_json::json!({"ipv4": "192.0.2.1", "ipv6": "2001:db8::1"}),
                false,
            ),
            (serde_json::json!({}), false),
        ] {
            assert_eq!(
                EndpointKeys::validate_cel(&endpoint, None).is_ok(),
                valid,
                "{endpoint}"
            );
        }
    }
}
