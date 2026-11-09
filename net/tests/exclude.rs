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
        exclude(net("10.0.0.0/24"), net("10.0.0.128/25")),
        nets(&["10.0.0.0/25"])
    );
    assert_eq!(
        exclude(net("10.0.0.0/24"), net("10.0.0.64/26")),
        nets(&["10.0.0.0/26", "10.0.0.128/25"])
    );
    assert_eq!(
        exclude(net("fd00::/62"), net("fd00:0:0:1::/64")),
        nets(&["fd00::/64", "fd00:0:0:2::/63"])
    );
}

#[test]
fn a_network_outside_changes_nothing_and_one_covering_leaves_nothing() {
    assert_eq!(
        exclude(net("10.0.0.0/24"), net("10.0.1.0/24")),
        nets(&["10.0.0.0/24"])
    );
    assert_eq!(
        exclude(net("10.0.0.0/24"), net("fd00::/8")),
        nets(&["10.0.0.0/24"])
    );
    assert_eq!(exclude(net("10.0.0.0/24"), net("10.0.0.0/24")), nets(&[]));
    assert_eq!(exclude(net("10.0.0.0/24"), net("10.0.0.0/8")), nets(&[]));
}

#[test]
fn host_bits_count_as_the_network_they_sit_in() {
    assert_eq!(
        exclude(net("10.0.0.7/24"), net("10.0.0.200/25")),
        nets(&["10.0.0.0/25"])
    );
}

/// Every subnet of a /28, at every prefix length: what is left plus what was excluded covers the
/// network once, address by address, with nothing outside it.
#[test]
fn the_rest_and_the_excluded_network_partition_the_network() {
    let whole = net("10.0.0.0/28");
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
