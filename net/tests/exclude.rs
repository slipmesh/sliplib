use sliplib_net::{IpNet, exclude};

fn nets(list: &[&str]) -> Vec<IpNet> {
    list.iter().map(|n| n.parse().unwrap()).collect()
}

fn net(n: &str) -> IpNet {
    n.parse().unwrap()
}

#[test]
fn leaves_the_rest_of_the_network_in_address_order() {
    assert_eq!(
        exclude(net("203.0.113.0/25"), net("203.0.113.64/26")),
        nets(&["203.0.113.0/26"])
    );
    assert_eq!(
        exclude(net("203.0.113.0/25"), net("203.0.113.32/27")),
        nets(&["203.0.113.0/27", "203.0.113.64/26"])
    );
    assert_eq!(
        exclude(net("3fff::/62"), net("3fff:0:0:1::/64")),
        nets(&["3fff::/64", "3fff:0:0:2::/63"])
    );
}

#[test]
fn a_network_outside_changes_nothing_and_one_covering_leaves_nothing() {
    assert_eq!(
        exclude(net("203.0.113.0/25"), net("203.0.113.128/25")),
        nets(&["203.0.113.0/25"])
    );
    assert_eq!(
        exclude(net("203.0.113.0/25"), net("2001:db8::/32")),
        nets(&["203.0.113.0/25"])
    );
    assert_eq!(
        exclude(net("203.0.113.0/25"), net("203.0.113.0/25")),
        nets(&[])
    );
    assert_eq!(
        exclude(net("203.0.113.0/25"), net("203.0.113.0/24")),
        nets(&[])
    );
}

#[test]
fn host_bits_count_as_the_network_they_sit_in() {
    assert_eq!(
        exclude(net("203.0.113.7/25"), net("203.0.113.100/26")),
        nets(&["203.0.113.0/26"])
    );
}

/// Every subnet of a /28, at every prefix length: what is left plus what was excluded covers the
/// network once, address by address, with nothing outside it.
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
