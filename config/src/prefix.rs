//! A prefix written as `"<address>/<prefix length>"`, host bits kept: `203.0.113.1/24` is an address
//! and its network.

use serde::{Deserialize, Serialize, Serializer};
use sliplib_net::{IpNet, Ipv4Net, Ipv6Net};

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct Prefix(pub IpNet);

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct Prefix4(pub Ipv4Net);

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct Prefix6(pub Ipv6Net);

macro_rules! prefix {
    ($type:ty, $parse:path) => {
        impl TryFrom<String> for $type {
            type Error = sliplib_net::ParseError;

            fn try_from(text: String) -> Result<Self, Self::Error> {
                $parse(&text).map(Self)
            }
        }

        impl Serialize for $type {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(&self.0)
            }
        }
    };
}

prefix!(Prefix, sliplib_net::parse);
prefix!(Prefix4, sliplib_net::parse_ipv4);
prefix!(Prefix6, sliplib_net::parse_ipv6);
