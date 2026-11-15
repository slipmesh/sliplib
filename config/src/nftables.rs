//! The `nftables` service's file that `slipmesh.yaml` names.

/// Where the template reaches the daemon - the `configFiles` entry of its `ExtensionServiceConfig`.
pub const TEMPLATE_PATH: &str = "/etc/talos-extensions/nftables.conf.j2";
