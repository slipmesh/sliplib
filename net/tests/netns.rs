//! Against the kernel, each test in a network namespace of its own. Run as the README's Development
//! section says.
#![cfg(target_os = "linux")]

#[path = "support/netns.rs"]
mod support;

use futures::StreamExt;
use sliplib_net::route::{self, MAIN_TABLE, Route, RtClient};
use sliplib_net::{Family, IpNet};
use std::net::IpAddr;
use std::time::Duration;
use support::{ip, own_network_namespace};

const TABLE: u32 = 152;
const PROTOCOL: u8 = 152;
/// Below BIRD's 32 in both families, as the egress agent sets it.
const METRIC: u32 = 16;

fn net(text: &str) -> IpNet {
    text.parse().unwrap()
}

fn address(text: &str) -> IpAddr {
    text.parse().unwrap()
}

/// Not from `/sys/class/net`: sysfs shows the namespace it was mounted in.
fn index(link: &str) -> u32 {
    let shown = ip(&format!("-o link show {link}"));
    shown.split_once(':').unwrap().0.parse().unwrap()
}

/// `link` up, with an address of each family.
fn uplink(link: &str) -> u32 {
    ip(&format!("link add {link} type dummy"));
    ip(&format!("link set {link} up"));
    ip(&format!("addr add 192.0.2.5/24 dev {link}"));
    ip(&format!("addr add 2001:db8::5/64 dev {link} nodad"));
    index(link)
}

fn ours(destination: &str, gateway: &str, oif: u32) -> Route {
    Route {
        destination: net(destination),
        table: TABLE,
        protocol: PROTOCOL,
        gateway: Some(address(gateway)),
        oif: Some(oif),
        metric: Some(METRIC),
    }
}

async fn listed(rt: &RtClient) -> Vec<Route> {
    let mut routes = rt.routes(TABLE, PROTOCOL).await.unwrap();
    routes.sort_by_key(|route| route.destination);
    routes
}

/// What BIRD leaves in the table: the same destinations, another protocol, its metric.
fn foreign_routes() {
    ip("-4 route add 198.51.100.0/24 via 192.0.2.1 dev up0 table 152 proto static metric 32");
    ip("-6 route add 3fff:1::/32 via 2001:db8::1 dev up0 table 152 proto static metric 32");
}

fn foreign_routes_left() -> String {
    ip("-4 route show table 152 proto static") + &ip("-6 route show table 152 proto static")
}

#[tokio::test]
#[ignore = "needs its own network namespace"]
async fn a_route_replaces_ours_beside_another_protocols_and_is_listed_by_table_and_protocol() {
    own_network_namespace();
    let up0 = uplink("up0");
    foreign_routes();
    ip("-4 route add 203.0.113.0/24 dev up0 proto 152");
    ip("-4 route add 203.0.113.0/24 dev up0 table 153 proto 152");
    let rt = RtClient::connect().unwrap();

    for route in [
        ours("198.51.100.0/24", "192.0.2.1", up0),
        ours("198.51.100.0/24", "192.0.2.2", up0),
        ours("3fff:1::/32", "2001:db8::1", up0),
    ] {
        rt.route_replace(&route).await.unwrap();
    }

    assert_eq!(
        listed(&rt).await,
        [
            ours("198.51.100.0/24", "192.0.2.2", up0),
            ours("3fff:1::/32", "2001:db8::1", up0),
        ]
    );
    let left = foreign_routes_left();
    assert!(
        left.contains("198.51.100.0/24") && left.contains("3fff:1::/32"),
        "{left}"
    );
}

#[tokio::test]
#[ignore = "needs its own network namespace"]
async fn a_delete_takes_ours_alone_a_missing_route_is_already_deleted_and_a_refusal_is_an_error() {
    own_network_namespace();
    let up0 = uplink("up0");
    foreign_routes();
    let rt = RtClient::connect().unwrap();
    let routes = [
        ours("198.51.100.0/24", "192.0.2.1", up0),
        ours("3fff:1::/32", "2001:db8::1", up0),
    ];
    for route in &routes {
        rt.route_replace(route).await.unwrap();
    }

    ip("-4 route add 203.0.113.0/24 dev up0 table 152 proto 152 metric 16 scope link");
    let linked = Route {
        destination: net("203.0.113.0/24"),
        table: TABLE,
        protocol: PROTOCOL,
        gateway: None,
        oif: Some(up0),
        metric: Some(METRIC),
    };
    assert!(listed(&rt).await.contains(&linked));

    for route in routes.iter().chain(&routes).chain([&linked]) {
        rt.route_del(route).await.unwrap();
    }

    assert_eq!(listed(&rt).await, []);
    let host_bits = ours("198.51.100.1/24", "192.0.2.1", up0);
    assert!(rt.route_del(&host_bits).await.is_err());
    let left = foreign_routes_left();
    assert!(
        left.contains("198.51.100.0/24") && left.contains("3fff:1::/32"),
        "{left}"
    );
}

#[tokio::test]
#[ignore = "needs its own network namespace"]
async fn a_route_on_a_link_alone_lands_in_main() {
    own_network_namespace();
    let up0 = uplink("up0");
    let rt = RtClient::connect().unwrap();
    let route = Route {
        destination: net("203.0.113.0/24"),
        table: MAIN_TABLE,
        protocol: 200,
        gateway: None,
        oif: Some(up0),
        metric: None,
    };

    rt.route_replace(&route).await.unwrap();

    assert_eq!(rt.routes(MAIN_TABLE, 200).await.unwrap(), [route]);
    assert_eq!(
        ip("-4 route show table main proto 200"),
        "203.0.113.0/24 dev up0 \n"
    );
}

#[tokio::test]
#[ignore = "needs its own network namespace"]
async fn the_default_route_is_the_main_tables_even_when_another_table_lists_one_first() {
    own_network_namespace();
    let main0 = uplink("main0");
    uplink("other0");
    let rt = RtClient::connect().unwrap();
    for (family, flag, gateway, any) in [
        (Family::Ipv4, "-4", "192.0.2.1", "0.0.0.0/0"),
        (Family::Ipv6, "-6", "2001:db8::1", "::/0"),
    ] {
        ip(&format!(
            "{flag} route add default via {gateway} dev other0 table 152"
        ));
        ip(&format!("{flag} route add default via {gateway} dev main0"));
        let listed = ip(&format!("{flag} route show table all default"));
        assert!(
            listed.starts_with(&format!("default via {gateway} dev other0 table 152")),
            "{listed}"
        );

        let default = rt.default_route(family).await.unwrap().unwrap();
        assert_eq!(
            (
                default.destination,
                default.table,
                default.gateway,
                default.oif
            ),
            (net(any), MAIN_TABLE, Some(address(gateway)), Some(main0))
        );
    }
}

#[tokio::test]
#[ignore = "needs its own network namespace"]
async fn an_uplink_has_the_addresses_of_its_family_on_its_own_interface() {
    own_network_namespace();
    for link in ["up0", "side0"] {
        ip(&format!("link add {link} type dummy"));
        ip(&format!("link set {link} up"));
    }
    ip("addr add 192.0.2.5/24 dev up0");
    ip("addr add 192.0.2.6/24 dev up0");
    ip("addr add 2001:db8::5/64 dev up0 nodad");
    ip("addr add 198.51.100.5/24 dev side0");
    ip("addr add 3fff::5/64 dev side0 nodad");
    ip("-4 route add default dev up0");
    ip("-6 route add default dev side0");
    let rt = RtClient::connect().unwrap();

    assert_eq!(
        rt.uplink(Family::Ipv4).await.unwrap(),
        Some((
            "up0".to_owned(),
            vec![address("192.0.2.5"), address("192.0.2.6")]
        ))
    );
    assert_eq!(
        rt.uplink(Family::Ipv6).await.unwrap(),
        Some(("side0".to_owned(), vec![address("3fff::5")]))
    );
}

#[tokio::test]
#[ignore = "needs its own network namespace"]
async fn a_route_of_either_family_wakes_whoever_follows_the_changes() {
    own_network_namespace();
    uplink("up0");
    let mut changes = route::link_route_and_address_changes().unwrap();
    for change in [
        "-4 route add 198.51.100.0/24 via 192.0.2.1",
        "-6 route add 3fff:1::/32 via 2001:db8::1",
    ] {
        while let Ok(change) =
            tokio::time::timeout(Duration::from_millis(100), changes.next()).await
        {
            assert_eq!(change, Some(()), "the changes ended");
        }
        ip(change);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), changes.next()).await,
            Ok(Some(())),
            "{change}"
        );
    }
}
