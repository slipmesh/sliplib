//! CIDR for every slipmesh component, on [`ipnet`]'s types. They are re-exported so that each
//! component takes them from here and one change reaches all of them.

use std::fmt;
use std::net::IpAddr;

pub use ipnet::{IpNet, Ipv4Net, Ipv6Net};

/// What is wrong with a CIDR read from a config, naming the input as it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

/// `"<address>/<prefix length>"` of either family. The address keeps its host bits: a config
/// gives an interface address and its network in one value, as `203.0.113.1/24`.
pub fn parse(cidr: &str) -> Result<IpNet, ParseError> {
    let (addr, prefix_len) = cidr
        .split_once('/')
        .ok_or_else(|| ParseError(format!("{cidr:?} is not a CIDR (missing '/')")))?;
    let addr: IpAddr = addr
        .parse()
        .map_err(|_| ParseError(format!("invalid address in {cidr:?}")))?;
    let prefix_len: u8 = prefix_len
        .parse()
        .map_err(|_| ParseError(format!("invalid prefix length in {cidr:?}")))?;
    let max = if addr.is_ipv4() { 32 } else { 128 };
    IpNet::new(addr, prefix_len).map_err(|_| {
        ParseError(format!(
            "invalid prefix length in {cidr:?}: {prefix_len} > {max}"
        ))
    })
}

/// [`parse`], for a value that may only be IPv4.
pub fn parse_ipv4(cidr: &str) -> Result<Ipv4Net, ParseError> {
    match parse(cidr)? {
        IpNet::V4(net) => Ok(net),
        IpNet::V6(_) => Err(ParseError(format!("{cidr:?} is not an IPv4 CIDR"))),
    }
}

/// [`parse`], for a value that may only be IPv6.
pub fn parse_ipv6(cidr: &str) -> Result<Ipv6Net, ParseError> {
    match parse(cidr)? {
        IpNet::V6(net) => Ok(net),
        IpNet::V4(_) => Err(ParseError(format!("{cidr:?} is not an IPv6 CIDR"))),
    }
}

/// What is left of `net` once `excluded` is taken out of it: the fewest networks that cover the
/// rest, in address order. A network outside `net`, or of the other family, leaves it whole;
/// one that covers it leaves nothing. Host bits count as the network they sit in.
pub fn exclude(net: IpNet, excluded: IpNet) -> Vec<IpNet> {
    let (net, excluded) = (net.trunc(), excluded.trunc());
    if excluded.contains(&net) {
        return Vec::new();
    }
    if !net.contains(&excluded) {
        return vec![net];
    }
    let mut rest = Vec::new();
    let mut current = net;
    while current != excluded {
        let mut halves = current.subnets(current.prefix_len() + 1).expect(
            "a network that strictly contains another is shorter than its family's maximum",
        );
        let (lower, upper) = (halves.next().unwrap(), halves.next().unwrap());
        if lower.contains(&excluded) {
            rest.push(upper);
            current = lower;
        } else {
            rest.push(lower);
            current = upper;
        }
    }
    rest.sort();
    rest
}

/// An address no host takes on an interface, because of the prefix it stands with there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reserved {
    /// IPv4, the host part of all zeros: it names the network (RFC 1812, section 4.2.3).
    Network,
    /// IPv4, the host part of all ones: the network's broadcast address (the same section).
    Broadcast,
    /// IPv6, the host part of all zeros: the Subnet-Router anycast address (RFC 4291, section
    /// 2.6.1), which every router on the link answers for.
    SubnetRouterAnycast,
}

/// What `address` is on an interface where it stands with its prefix length, if it is one no host
/// takes there. A point-to-point prefix, an IPv4 /31 (RFC 3021) or an IPv6 /127 (RFC 6164), and a
/// single address, /32 or /128, have no such address.
pub fn reserved(address: IpNet) -> Option<Reserved> {
    match address {
        IpNet::V4(v4) if v4.prefix_len() < 31 => {
            if v4.addr() == v4.network() {
                Some(Reserved::Network)
            } else if v4.addr() == v4.broadcast() {
                Some(Reserved::Broadcast)
            } else {
                None
            }
        }
        IpNet::V6(v6) if v6.prefix_len() < 127 && v6.addr() == v6.network() => {
            Some(Reserved::SubnetRouterAnycast)
        }
        _ => None,
    }
}
