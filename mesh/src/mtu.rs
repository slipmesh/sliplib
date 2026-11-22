//! The one MTU every mesh interface gets: the largest at which a full-size packet, with its
//! WireGuard and junk overhead, still crosses every link's underlay unfragmented.
//!
//! A link's limit is `U - 32 - 8 - IP - S4`. `U` is the smaller uplink MTU of its two nodes, 32 is
//! WireGuard's own header and tag, 8 the UDP header, `IP` 20 or 40 for the underlay family, and S4
//! the junk AmneziaWG adds to every data packet. WireGuard pads a packet to a multiple of 16, but
//! never past the MTU, so a full-size packet carries no padding. A fragmented outer packet takes
//! TCP in the tunnel down to a few Mbit/s, which is what this guards against.

use crate::model::{Family, Link, Network, Node, UnknownNode};

/// A node's underlay MTU when the files set none: that of Ethernet.
pub(crate) const DEFAULT_UPLINK_MTU: u32 = 1500;

/// The least MTU IPv6 allows a link (RFC 8200); the mesh carries OSPFv3 over IPv6 link-locals.
pub(crate) const MIN_MESH_MTU: u32 = 1280;

/// Why the mesh has no MTU: every node's interfaces take the same one, so this stops them all.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MtuError {
    #[error("mesh link {link}")]
    Link {
        link: String,
        #[source]
        source: LinkError,
    },
    #[error("mesh.mtu is set, but the mesh has no link to give it to")]
    NoLink,
    #[error("mesh.mtu {mtu} is above {limit}, the limit of mesh link {link}")]
    AboveLimit { mtu: u32, limit: u32, link: String },
    #[error("mesh.mtu {mtu} is below {MIN_MESH_MTU}, the least IPv6 allows a link")]
    BelowMinimum { mtu: u32 },
    #[error(
        "mesh link {link} limits the mesh MTU to {limit}, below {MIN_MESH_MTU}, the least IPv6 allows a link - raise the underlay `mtu` of its narrower end, or lower its `s4`"
    )]
    LimitBelowMinimum { link: String, limit: u32 },
}

/// Why a link has no limit to count.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LinkError {
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
    #[error(
        "the endpoints of {0:?} and {1:?} differ in address family, so which one carries the link depends on who starts the handshake"
    )]
    FamiliesDiffer(String, String),
    #[error("neither {0:?} nor {1:?} has an endpoint, so neither can dial the other")]
    NoEndpoint(String, String),
}

/// The MTU of every mesh interface: the one the files set for the mesh when it fits every link,
/// else the least link limit. None for a mesh without links, which has no mesh interface to give
/// it to.
pub fn mesh_mtu(mesh: &Network) -> Result<Option<u32>, MtuError> {
    let mut least: Option<(u32, &Link)> = None;
    for link in &mesh.links {
        let limit = link_limit(mesh, link).map_err(|source| MtuError::Link {
            link: link.name.clone(),
            source,
        })?;
        if least.is_none_or(|(least, _)| limit < least) {
            least = Some((limit, link));
        }
    }
    let Some((limit, link)) = least else {
        if mesh.mtu.is_some() {
            return Err(MtuError::NoLink);
        }
        return Ok(None);
    };
    if limit < MIN_MESH_MTU {
        return Err(MtuError::LimitBelowMinimum {
            link: link.name.clone(),
            limit,
        });
    }
    let Some(mtu) = mesh.mtu else {
        return Ok(Some(limit));
    };
    if mtu > limit {
        return Err(MtuError::AboveLimit {
            mtu,
            limit,
            link: link.name.clone(),
        });
    }
    if mtu < MIN_MESH_MTU {
        return Err(MtuError::BelowMinimum { mtu });
    }
    Ok(Some(mtu))
}

/// The largest MTU at which a full-size packet crosses `link` unfragmented.
fn link_limit(mesh: &Network, link: &Link) -> Result<u32, LinkError> {
    let [a, b] = &link.ends;
    let (a, b) = (mesh.node(a)?, mesh.node(b)?);
    let ip_header = match link_family(a, b)? {
        Family::Ipv4 => 20,
        Family::Ipv6 => 40,
    };
    let uplink = a.underlay_mtu.min(b.underlay_mtu);
    let junk = link.obfuscation.s4.map_or(0, u32::from);
    // An underlay the link's bytes fill leaves 0, which `mesh_mtu` refuses as any limit below
    // 1280, as it does the limit of an underlay below its family's least.
    Ok(uplink.saturating_sub(32 + 8 + ip_header + junk))
}

/// The family of the endpoint a link is dialed at: either side's, and the same when both have one.
fn link_family(a: &Node, b: &Node) -> Result<Family, LinkError> {
    let family = |node: &Node| node.endpoint.as_ref().map(|endpoint| endpoint.family);
    match (family(a), family(b)) {
        (Some(x), Some(y)) if x != y => {
            Err(LinkError::FamiliesDiffer(a.name.clone(), b.name.clone()))
        }
        (Some(family), _) | (None, Some(family)) => Ok(family),
        (None, None) => Err(LinkError::NoEndpoint(a.name.clone(), b.name.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The outer packet as it leaves the node: the inner packet padded to 16 but never past the
    /// MTU, then the WireGuard header and tag, the junk, UDP and the underlay's IP header.
    fn outer(inner: u32, mtu: u32, s4: u32, ip: u32) -> u32 {
        inner.div_ceil(16).saturating_mul(16).min(mtu) + 32 + s4 + 8 + ip
    }

    /// A network of `mesh_and_nodes`, its `mesh` and `nodes`, under one `slipmesh:` block, its
    /// addressing and its AS number.
    fn lowered(mesh_and_nodes: &str) -> Network {
        let text = format!(
            "slipmesh: {{kind: network, version: 2, revision: 1}}\naddressing: {{loopback: \
             {{ipv4: 198.51.100.0/25, ipv6: '3fff::/64'}}}}\nrouting: {{bgp: {{as: 64512}}}}\n\
             {mesh_and_nodes}"
        );
        crate::v2::lower(&sliplib_config::v2::File::parse(&text).unwrap()).unwrap()
    }

    /// Two nodes `a` and `b` and the link `a-b` between them: `a` and `b` are their underlay
    /// entries, `link` the link's keys after its ends, `mesh` keys of `mesh`.
    fn network(a: &str, b: &str, link: &str, mesh: &str) -> Network {
        lowered(&format!(
            "mesh: {{interface_prefix: mesh-, links: {{a-b: {{ends: {{a: {{}}, b: {{}}}}, \
             {link}}}}}{mesh}}}\nnodes:\n  a: {{platform: talos, node_id: 0.0.0.1, underlay: \
             {{wan: {a}}}}}\n  b: {{platform: talos, node_id: 0.0.0.2, underlay: {{wan: {b}}}}}\n"
        ))
    }

    fn of(network: &Network) -> Result<Option<u32>, MtuError> {
        mesh_mtu(network)
    }

    fn mtu_of(a: &str, b: &str, link: &str, mesh: &str) -> u32 {
        of(&network(a, b, link, mesh)).unwrap().unwrap()
    }

    fn refusal_of(a: &str, b: &str, link: &str, mesh: &str) -> MtuError {
        of(&network(a, b, link, mesh)).unwrap_err()
    }

    fn link_refusal_of(a: &str, b: &str, link: &str) -> LinkError {
        match refusal_of(a, b, link, "") {
            MtuError::Link { link, source } if link == "a-b" => source,
            other => panic!("refused for its link: {other:?}"),
        }
    }

    const WIREGUARD: &str = "type: wireguard, wireguard: {port: 51820}";
    const AT_V4: &str = "{endpoint: {ipv4: 192.0.2.10}}";
    const AT_V4_OTHER: &str = "{endpoint: {ipv4: 192.0.2.11}}";
    const AT_V6: &str = "{endpoint: {ipv6: '2001:db8::10'}}";
    const UNDIALED: &str = "{}";

    fn amneziawg(obfuscation: &str) -> String {
        format!("type: amneziawg, amneziawg: {{port: 51820, obfuscation: {obfuscation}}}")
    }

    #[test]
    fn a_link_dialed_over_ipv4_on_a_1500_underlay_takes_1440_and_over_ipv6_20_less() {
        assert_eq!(mtu_of(AT_V4, AT_V4_OTHER, WIREGUARD, ""), 1440);
        assert_eq!(mtu_of(AT_V6, UNDIALED, WIREGUARD, ""), 1420);
    }

    #[test]
    fn a_link_is_dialed_over_the_family_of_whichever_end_has_an_endpoint() {
        assert_eq!(mtu_of(AT_V4, UNDIALED, WIREGUARD, ""), 1440);
        assert_eq!(mtu_of(UNDIALED, AT_V4, WIREGUARD, ""), 1440);
        assert_eq!(mtu_of(UNDIALED, AT_V6, WIREGUARD, ""), 1420);
    }

    #[test]
    fn a_host_name_is_dialed_over_the_family_of_its_key() {
        let named = |family: &str| format!("{{endpoint: {{{family}: a.example.com}}}}");
        assert_eq!(mtu_of(&named("ipv4"), UNDIALED, WIREGUARD, ""), 1440);
        assert_eq!(mtu_of(&named("ipv6"), UNDIALED, WIREGUARD, ""), 1420);
    }

    #[test]
    fn ends_dialed_over_two_families_or_neither_dialed_refuse_the_link_naming_its_ends() {
        let differ = link_refusal_of(AT_V4, AT_V6, WIREGUARD);
        assert!(
            matches!(&differ, LinkError::FamiliesDiffer(a, b) if a == "a" && b == "b"),
            "{differ:?}"
        );
        let neither = link_refusal_of(UNDIALED, UNDIALED, WIREGUARD);
        assert!(
            matches!(&neither, LinkError::NoEndpoint(a, b) if a == "a" && b == "b"),
            "{neither:?}"
        );
    }

    #[test]
    fn the_smaller_underlay_of_the_two_ends_bounds_the_link() {
        let at_v4 = |mtu: u32| format!("{{mtu: {mtu}, endpoint: {{ipv4: 192.0.2.10}}}}");
        assert_eq!(mtu_of(&at_v4(1460), UNDIALED, WIREGUARD, ""), 1400);
        assert_eq!(mtu_of(AT_V4, "{mtu: 1460}", WIREGUARD, ""), 1400);
        assert_eq!(mtu_of(&at_v4(1460), "{mtu: 1450}", WIREGUARD, ""), 1390);
    }

    #[test]
    fn the_junk_on_data_packets_lowers_the_link_by_its_size_its_own_or_its_types() {
        assert_eq!(mtu_of(AT_V6, UNDIALED, &amneziawg("{s4: 12}"), ""), 1408);
        let of_the_type = ", amneziawg: {obfuscation: {s4: 12}}";
        assert_eq!(mtu_of(AT_V4, UNDIALED, &amneziawg("{}"), of_the_type), 1428);
        assert_eq!(
            mtu_of(AT_V4, UNDIALED, &amneziawg("{s4: 20}"), of_the_type),
            1420
        );
    }

    #[test]
    fn the_mesh_takes_the_least_limit_of_its_links() {
        // a-b dials a over IPv4 at 1500: 1440. b-c dials c over IPv6 at 1490: 1410.
        let network = lowered(
            "mesh:\n  interface_prefix: mesh-\n  links:\n    a-b: {type: wireguard, ends: \
             {a: {}, b: {}}, wireguard: {port: 51820}}\n    b-c: {type: wireguard, ends: {b: {}, \
             c: {}}, wireguard: {port: 51821}}\nnodes:\n  a: {platform: talos, node_id: 0.0.0.1, \
             underlay: {wan: {endpoint: {ipv4: 192.0.2.10}}}}\n  b: {platform: talos, node_id: \
             0.0.0.2, underlay: {wan: {}}}\n  c: {platform: talos, node_id: 0.0.0.3, underlay: \
             {wan: {mtu: 1490, endpoint: {ipv6: '2001:db8::12'}}}}\n",
        );
        assert_eq!(of(&network).unwrap(), Some(1410));
    }

    #[test]
    fn a_mesh_mtu_at_or_below_the_least_limit_is_taken_and_one_above_is_refused() {
        assert_eq!(mtu_of(AT_V4, UNDIALED, WIREGUARD, ", mtu: 1420"), 1420);
        assert_eq!(mtu_of(AT_V4, UNDIALED, WIREGUARD, ", mtu: 1440"), 1440);
        let above = refusal_of(AT_V4, UNDIALED, WIREGUARD, ", mtu: 1441");
        assert!(
            matches!(&above, MtuError::AboveLimit { mtu: 1441, limit: 1440, link }
                if link == "a-b"),
            "{above:?}"
        );
    }

    #[test]
    fn a_limit_or_a_mesh_mtu_below_1280_is_refused_and_1280_is_not() {
        assert_eq!(mtu_of(AT_V4, "{mtu: 1340}", WIREGUARD, ""), 1280);
        let limit = refusal_of(AT_V4, "{mtu: 1339}", WIREGUARD, "");
        assert!(
            matches!(&limit, MtuError::LimitBelowMinimum { link, limit: 1279 } if link == "a-b"),
            "{limit:?}"
        );
        let set = refusal_of(AT_V4, UNDIALED, WIREGUARD, ", mtu: 1279");
        assert!(
            matches!(set, MtuError::BelowMinimum { mtu: 1279 }),
            "{set:?}"
        );
    }

    #[test]
    fn a_mesh_without_links_takes_no_mtu_and_refuses_one_set() {
        let unlinked = |mesh: &str| {
            lowered(&format!(
                "mesh: {{interface_prefix: mesh-{mesh}}}\nnodes:\n  a: {{platform: talos, \
                 node_id: 0.0.0.1, underlay: {{wan: {{}}}}}}\n"
            ))
        };
        assert_eq!(of(&unlinked("")).unwrap(), None);
        let refusal = of(&unlinked(", mtu: 1400")).unwrap_err();
        assert!(matches!(refusal, MtuError::NoLink), "{refusal:?}");
    }

    #[test]
    fn an_underlay_below_its_familys_least_or_with_no_room_left_gives_its_link_a_refused_limit() {
        // Below 1280 for IPv6 or 68 for IPv4, an underlay leaves a limit under 1280 whatever the
        // link adds; one the link's bytes reach leaves 0.
        for (a, b, link, limit) in [
            (AT_V6, "{mtu: 1279}", WIREGUARD.to_owned(), 1199),
            (AT_V4, "{mtu: 67}", WIREGUARD.to_owned(), 7),
            (AT_V6, UNDIALED, amneziawg("{s4: 1420}"), 0),
            (AT_V6, UNDIALED, amneziawg("{s4: 1500}"), 0),
        ] {
            for mesh in ["", ", mtu: 1280", ", mtu: 1"] {
                let refusal = refusal_of(a, b, &link, mesh);
                assert!(
                    matches!(&refusal, MtuError::LimitBelowMinimum { link, limit: l }
                        if link == "a-b" && *l == limit),
                    "{b} {link} {mesh}: {refusal:?}"
                );
            }
        }
    }

    #[test]
    fn across_the_grid_a_full_size_packet_fits_and_one_byte_more_does_not() {
        for s4 in [0, 12, 20, 89, 150] {
            for underlay in [1400, 1460, 1500, 9000] {
                for (endpoint, ip) in [("ipv4: 192.0.2.10", 20), ("ipv6: '2001:db8::10'", 40)] {
                    let a = format!("{{mtu: {underlay}, endpoint: {{{endpoint}}}}}");
                    let b = format!("{{mtu: {underlay}}}");
                    let link = amneziawg(&format!("{{s4: {s4}}}"));
                    let Ok(Some(mtu)) = of(&network(&a, &b, &link, "")) else {
                        assert!(underlay - 32 - 8 - ip - s4 < MIN_MESH_MTU);
                        continue;
                    };
                    assert!(outer(mtu, mtu, s4, ip) <= underlay, "{s4} {underlay} {ip}");
                    assert!(
                        outer(mtu + 1, mtu + 1, s4, ip) > underlay,
                        "{s4} {underlay} {ip}"
                    );
                }
            }
        }
    }
}
