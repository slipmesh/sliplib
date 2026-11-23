//! The mesh MTU as the nodes and the cluster know it.

/// RFC 8200: the least MTU of a link that carries IPv6, as every link of the mesh does.
pub const MIN_IPV6_MTU: u32 = 1280;

/// The annotation on a node's `Node` object that carries the MTU of the mesh.
pub const MESH_MTU_ANNOTATION: &str = "slipmesh.net/mesh_mtu";
