//! CIDR on [`ipnet`]'s types, re-exported so one version reaches every component, and the mesh
//! MTU's floor and annotation.

pub mod mtu;

use std::fmt;
use std::net::IpAddr;

pub use ipnet::{IpNet, Ipv4Net, Ipv6Net};

/// Names the input as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

/// `"<address>/<prefix length>"`, host bits kept: `203.0.113.1/24` is an address and its network.
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

pub fn parse_ipv4(cidr: &str) -> Result<Ipv4Net, ParseError> {
    match parse(cidr)? {
        IpNet::V4(net) => Ok(net),
        IpNet::V6(_) => Err(ParseError(format!("{cidr:?} is not an IPv4 CIDR"))),
    }
}

pub fn parse_ipv6(cidr: &str) -> Result<Ipv6Net, ParseError> {
    match parse(cidr)? {
        IpNet::V6(net) => Ok(net),
        IpNet::V4(_) => Err(ParseError(format!("{cidr:?} is not an IPv6 CIDR"))),
    }
}

/// The fewest networks covering `net` less `excluded`, in address order. Host bits count as their
/// network.
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

/// The fewest networks covering every address in `included` and in none of `excluded`, IPv4 first,
/// then IPv6, each in address order. Host bits count as their network.
pub fn subtract(included: &[IpNet], excluded: &[IpNet]) -> Vec<IpNet> {
    // Sorted and disjoint, so the cuts of one network are a run of them.
    let excluded = IpNet::aggregate(&excluded.to_vec());
    let mut rest = Vec::new();
    for net in IpNet::aggregate(&included.to_vec()) {
        let first = excluded.partition_point(|cut| cut.broadcast() < net.network());
        let mut pieces = vec![net];
        for &cut in excluded[first..]
            .iter()
            .take_while(|cut| cut.network() <= net.broadcast())
        {
            pieces = pieces
                .into_iter()
                .flat_map(|piece| exclude(piece, cut))
                .collect();
        }
        rest.extend(pieces);
    }
    rest
}
