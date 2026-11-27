//! The annotations of a node's `Node` object that carry the mesh's facts, as
//! [the format](../../docs/slipmesh-yaml.md) names them: one spelling for the tool that writes
//! them and the agents that read them.

pub use sliplib_net::mtu::MESH_MTU_ANNOTATION as MESH_MTU;

pub const NAMESPACE: &str = "slipmesh.net/";
pub const INTERFACE_PREFIX_MESH: &str = "slipmesh.net/interface_prefix.mesh";
pub const ENDPOINT: &str = "slipmesh.net/endpoint";

/// The kernel protocol number of the routes of `class`.
pub fn route_proto_id(class: &str) -> String {
    format!("{NAMESPACE}route_proto_id.{class}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_class_names_its_own_protocol_annotation() {
        assert_eq!(
            route_proto_id("egress"),
            "slipmesh.net/route_proto_id.egress"
        );
    }
}
