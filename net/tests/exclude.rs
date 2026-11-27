use sliplib_net::{IpNet, exclude};

fn nets(list: &[&str]) -> Vec<IpNet> {
    list.iter().map(|n| n.parse().unwrap()).collect()
}

fn net(n: &str) -> IpNet {
    n.parse().unwrap()
}

#[test]
fn leaves_the_rest_of_the_network_in_address_order() {
    for (network, excluded, rest) in [
        ("203.0.113.0/25", "203.0.113.64/26", &["203.0.113.0/26"][..]),
        (
            "203.0.113.0/25",
            "203.0.113.32/27",
            &["203.0.113.0/27", "203.0.113.64/26"],
        ),
        (
            "3fff::/62",
            "3fff:0:0:1::/64",
            &["3fff::/64", "3fff:0:0:2::/63"],
        ),
        // One outside changes nothing, one covering leaves nothing.
        ("203.0.113.0/25", "203.0.113.128/25", &["203.0.113.0/25"]),
        ("203.0.113.0/25", "2001:db8::/32", &["203.0.113.0/25"]),
        ("203.0.113.0/25", "203.0.113.0/25", &[]),
        ("203.0.113.0/25", "203.0.113.0/24", &[]),
        // Host bits count as the network they sit in.
        ("203.0.113.7/25", "203.0.113.100/26", &["203.0.113.0/26"]),
    ] {
        assert_eq!(
            exclude(net(network), net(excluded)),
            nets(rest),
            "{network} without {excluded}"
        );
    }
}

/// Every subnet of a /28: the rest and the excluded cover it once, address by address.
#[test]
fn the_rest_and_the_excluded_network_partition_the_network() {
    let whole = net("203.0.113.0/28");
    for len in 28..=32 {
        for excluded in whole.subnets(len).unwrap() {
            let rest = exclude(whole, excluded);
            for host in whole.hosts().chain([whole.network(), whole.broadcast()]) {
                let covering = rest.iter().filter(|n| n.contains(&host)).count()
                    + usize::from(excluded.contains(&host));
                assert_eq!(covering, 1, "{host} in {whole} minus {excluded}: {rest:?}");
            }
            assert!(rest.iter().all(|n| whole.contains(n)), "{rest:?}");
        }
    }
}
