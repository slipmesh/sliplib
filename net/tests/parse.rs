use sliplib_net::{IpNet, parse, parse_ipv4, parse_ipv6};

#[test]
fn keeps_the_host_address_of_either_family() {
    let v4 = parse("203.0.113.1/24").unwrap();
    assert_eq!(
        (v4.addr().to_string(), v4.prefix_len()),
        ("203.0.113.1".into(), 24)
    );
    let v6 = parse("fe80::1/64").unwrap();
    assert_eq!(
        (v6.addr().to_string(), v6.prefix_len()),
        ("fe80::1".into(), 64)
    );
}

#[test]
fn errors_name_the_input_and_what_is_wrong_with_it() {
    for (input, error) in [
        (
            "203.0.113.1",
            r#""203.0.113.1" is not a CIDR (missing '/')"#,
        ),
        ("203.0.113.x/24", r#"invalid address in "203.0.113.x/24""#),
        (
            "203.0.113.1/x",
            r#"invalid prefix length in "203.0.113.1/x""#,
        ),
        (
            "203.0.113.1/33",
            r#"invalid prefix length in "203.0.113.1/33": 33 > 32"#,
        ),
        (
            "fe80::1/129",
            r#"invalid prefix length in "fe80::1/129": 129 > 128"#,
        ),
    ] {
        assert_eq!(parse(input).unwrap_err().to_string(), error);
    }
}

#[test]
fn ipv4_parsing_takes_ipv4_only() {
    let net = parse_ipv4("203.0.113.5/24").unwrap();
    assert_eq!(IpNet::V4(net), parse("203.0.113.5/24").unwrap());
    assert_eq!(
        parse_ipv4("3fff::1/64").unwrap_err().to_string(),
        r#""3fff::1/64" is not an IPv4 CIDR"#
    );
    assert_eq!(
        parse_ipv4("203.0.113.1").unwrap_err().to_string(),
        r#""203.0.113.1" is not a CIDR (missing '/')"#
    );
}

#[test]
fn ipv6_parsing_takes_ipv6_only() {
    let net = parse_ipv6("3fff::1/64").unwrap();
    assert_eq!(IpNet::V6(net), parse("3fff::1/64").unwrap());
    assert_eq!(
        parse_ipv6("203.0.113.5/24").unwrap_err().to_string(),
        r#""203.0.113.5/24" is not an IPv6 CIDR"#
    );
    assert_eq!(
        parse_ipv6("3fff::1").unwrap_err().to_string(),
        r#""3fff::1" is not a CIDR (missing '/')"#
    );
}
