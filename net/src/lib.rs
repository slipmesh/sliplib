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
/// gives an interface address and its network in one value, as `10.0.0.1/24`.
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
