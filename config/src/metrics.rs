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

/// What is wrong with where a daemon would serve its metrics. `field` is the caller's own path
/// to the section, so the message points at the document the reader is holding.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MetricsError {
    #[error("{field} {listen:?} is not an ip:port: {error}")]
    NotAnAddress {
        field: String,
        listen: String,
        error: std::net::AddrParseError,
    },
    #[error(
        "{field} {listen:?} is not IPv4 - Prometheus discovers a node by its InternalIP, which is the v4 loopback"
    )]
    NotIpv4 { field: String, listen: String },
    #[error(
        "{field} {listen:?} is a wildcard address - bind this node's own mesh loopback, which exists only inside the overlay"
    )]
    Wildcard { field: String, listen: String },
    #[error("{field} {listen:?} has no port - omit the section to serve nothing")]
    NoPort { field: String, listen: String },
}

impl MetricsConfig {
    /// `field` names the caller's own path to this section.
    pub fn validate(&self, field: &str) -> Result<(), MetricsError> {
        let (field, listen) = (field.to_owned(), self.listen.clone());
        let address: std::net::SocketAddr = match self.listen.parse() {
            Ok(address) => address,
            Err(error) => {
                return Err(MetricsError::NotAnAddress {
                    field,
                    listen,
                    error,
                });
            }
        };
        if !address.is_ipv4() {
            return Err(MetricsError::NotIpv4 { field, listen });
        }
        // Both of the below parse happily and would bind, which is why they are worth refusing
        // here. A wildcard address puts the endpoint on every interface the node has, public ones
        // included, when being reachable only inside the overlay is its whole protection.
        if address.ip().is_unspecified() {
            return Err(MetricsError::Wildcard { field, listen });
        }
        // Port 0 asks the kernel for an ephemeral one, which nothing can then be pointed at.
        // "Serve nothing" is spelled by omitting the section.
        if address.port() == 0 {
            return Err(MetricsError::NoPort { field, listen });
        }
        Ok(())
    }
}
