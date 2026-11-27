use sliplib_net::{IpNet, subtract};

fn nets(list: &[&str]) -> Vec<IpNet> {
    list.iter().map(|n| n.parse().unwrap()).collect()
}

#[test]
fn every_excluded_network_leaves_every_included_one_of_its_family_and_the_rest_merges() {
    assert_eq!(
        subtract(
            &nets(&[
                "203.0.113.0/25",
                "203.0.113.128/25",
                "198.51.100.0/24",
                "3fff::/62",
            ]),
            &nets(&["203.0.113.64/26", "198.51.100.0/24", "3fff:0:0:1::/64"]),
        ),
        nets(&[
            "203.0.113.0/26",
            "203.0.113.128/25",
            "3fff::/64",
            "3fff:0:0:2::/63",
        ])
    );
}

/// Every pair of included and every pair of excluded subnets of a /28, down to /30.
#[test]
fn the_rest_covers_each_address_included_and_not_excluded_with_the_fewest_networks() {
    let whole: IpNet = "203.0.113.0/28".parse().unwrap();
    let parts: Vec<IpNet> = (28..=30)
        .flat_map(|len| whole.subnets(len).unwrap())
        .collect();
    let pairs: Vec<[IpNet; 2]> = parts
        .iter()
        .flat_map(|&a| parts.iter().map(move |&b| [a, b]))
        .collect();
    for included in &pairs {
        for excluded in &pairs {
            let rest = subtract(included, excluded);
            for host in whole.hosts().chain([whole.network(), whole.broadcast()]) {
                let expected = included.iter().any(|n| n.contains(&host))
                    && !excluded.iter().any(|n| n.contains(&host));
                let covering = rest.iter().filter(|n| n.contains(&host)).count();
                assert_eq!(
                    covering,
                    usize::from(expected),
                    "{host} in {included:?} minus {excluded:?}: {rest:?}"
                );
            }
            assert_eq!(
                rest,
                IpNet::aggregate(&rest),
                "{included:?} minus {excluded:?}"
            );
        }
    }
}
