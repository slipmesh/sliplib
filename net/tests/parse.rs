use sliplib_net::{IpNet, parse, parse_ipv4};

#[test]
fn keeps_the_host_address_of_either_family() {
    let v4 = parse("10.0.0.1/24").unwrap();
    assert_eq!(
        (v4.addr().to_string(), v4.prefix_len()),
        ("10.0.0.1".into(), 24)
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
        ("10.0.0.1", r#""10.0.0.1" is not a CIDR (missing '/')"#),
        ("10.0.0.x/24", r#"invalid address in "10.0.0.x/24""#),
        ("10.0.0.1/x", r#"invalid prefix length in "10.0.0.1/x""#),
        (
            "10.0.0.1/33",
            r#"invalid prefix length in "10.0.0.1/33": 33 > 32"#,
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
    let net = parse_ipv4("192.168.88.5/24").unwrap();
    assert_eq!(IpNet::V4(net), parse("192.168.88.5/24").unwrap());
    assert_eq!(
        parse_ipv4("fd00::1/64").unwrap_err().to_string(),
        r#""fd00::1/64" is not an IPv4 CIDR"#
    );
    assert_eq!(
        parse_ipv4("10.0.0.1").unwrap_err().to_string(),
        r#""10.0.0.1" is not a CIDR (missing '/')"#
    );
}
