use sliplib_net::{Reserved, parse, reserved};

fn on_its_interface(address: &str) -> Option<Reserved> {
    reserved(parse(address).unwrap())
}

#[test]
fn an_ipv4_prefix_shorter_than_31_keeps_its_network_and_broadcast_addresses() {
    assert_eq!(on_its_interface("203.0.113.0/24"), Some(Reserved::Network));
    assert_eq!(
        on_its_interface("203.0.113.255/24"),
        Some(Reserved::Broadcast)
    );
    assert_eq!(on_its_interface("203.0.113.64/30"), Some(Reserved::Network));
    assert_eq!(
        on_its_interface("203.0.113.67/30"),
        Some(Reserved::Broadcast)
    );
    for host in ["203.0.113.1/24", "203.0.113.254/24", "203.0.113.65/30"] {
        assert_eq!(on_its_interface(host), None, "{host}");
    }
}

#[test]
fn both_addresses_of_an_ipv4_31_and_the_one_of_a_32_are_hosts() {
    for host in ["203.0.113.0/31", "203.0.113.1/31", "203.0.113.0/32"] {
        assert_eq!(on_its_interface(host), None, "{host}");
    }
}

#[test]
fn an_ipv6_prefix_shorter_than_127_keeps_its_subnet_router_anycast_address() {
    assert_eq!(
        on_its_interface("2001:db8::/64"),
        Some(Reserved::SubnetRouterAnycast)
    );
    assert_eq!(
        on_its_interface("2001:db8::4/126"),
        Some(Reserved::SubnetRouterAnycast)
    );
    // IPv6 has no broadcast: the host part of all ones is a host.
    for host in [
        "2001:db8::1/64",
        "2001:db8::ffff:ffff:ffff:ffff/64",
        "2001:db8::7/126",
    ] {
        assert_eq!(on_its_interface(host), None, "{host}");
    }
}

#[test]
fn both_addresses_of_an_ipv6_127_and_the_one_of_a_128_are_hosts() {
    for host in ["2001:db8::/127", "2001:db8::1/127", "2001:db8::/128"] {
        assert_eq!(on_its_interface(host), None, "{host}");
    }
}
