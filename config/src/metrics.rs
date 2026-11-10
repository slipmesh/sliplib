/// Where a daemon serves its Prometheus endpoint. Shared rather than restated per daemon: the
/// address is the same kind of thing in each, and the generator derives every one of them the same
/// way from the node's own loopback.
#[derive(serde::Deserialize, serde::Serialize, Debug, PartialEq, Clone)]
pub struct MetricsConfig {
    /// `ip:port` - the node's own v4 mesh loopback, never `0.0.0.0`: that address exists only
    /// inside the overlay, so the endpoint is unreachable from outside without depending on a
    /// firewall rule being in place first. IPv4 because that is the address kubelet reports as
    /// `InternalIP`, which is how Prometheus discovers it.
    pub listen: String,
}

impl MetricsConfig {
    /// `field` names the caller's own path to this section, so the message points at the document
    /// the reader is holding rather than at this crate.
    pub fn validate(&self, field: &str) -> anyhow::Result<()> {
        let listen: std::net::SocketAddr = self
            .listen
            .parse()
            .map_err(|e| anyhow::anyhow!("{field} {:?} is not an ip:port: {e}", self.listen))?;
        anyhow::ensure!(
            listen.is_ipv4(),
            "{field} {:?} is not IPv4 - Prometheus discovers a node by its InternalIP, which is \
             the v4 loopback",
            self.listen
        );
        // Both of the below parse happily and would bind, which is why they are worth refusing
        // here. A wildcard address puts the endpoint on every interface the node has, public ones
        // included, when being reachable only inside the overlay is its whole protection.
        anyhow::ensure!(
            !listen.ip().is_unspecified(),
            "{field} {:?} is a wildcard address - bind this node's own mesh loopback, which \
             exists only inside the overlay",
            self.listen
        );
        // Port 0 asks the kernel for an ephemeral one, which nothing can then be pointed at.
        // "Serve nothing" is spelled by omitting the section.
        anyhow::ensure!(
            listen.port() != 0,
            "{field} {:?} has no port - omit the section to serve nothing",
            self.listen
        );
        Ok(())
    }
}
