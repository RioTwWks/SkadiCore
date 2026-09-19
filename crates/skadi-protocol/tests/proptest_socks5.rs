//! Property-based тесты SOCKS5-парсеров.

mod proptest_common;

use proptest::prelude::*;
use proptest_common::{assert_consumed_in_bounds, assert_incomplete_invariant, valid_domain};
use skadi_core::Endpoint;
use skadi_protocol::socks5::parse::{parse_auth, parse_greeting, parse_request, ParseError};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

fn encode_greeting(methods: &[u8]) -> Vec<u8> {
    let mut buf = vec![0x05, methods.len() as u8];
    buf.extend_from_slice(methods);
    buf
}

fn encode_auth(user: &str, pass: &str) -> Vec<u8> {
    let mut buf = vec![0x01, user.len() as u8];
    buf.extend_from_slice(user.as_bytes());
    buf.push(pass.len() as u8);
    buf.extend_from_slice(pass.as_bytes());
    buf
}

fn encode_connect(endpoint: &Endpoint) -> Vec<u8> {
    let mut buf = vec![0x05, 0x01, 0x00];
    match endpoint {
        Endpoint::Ip(addr) => match addr.ip() {
            IpAddr::V4(ip) => {
                buf.push(0x01);
                buf.extend_from_slice(&ip.octets());
            }
            IpAddr::V6(ip) => {
                buf.push(0x04);
                buf.extend_from_slice(&ip.octets());
            }
        },
        Endpoint::Domain(domain, _) => {
            let bytes = domain.as_bytes();
            buf.push(0x03);
            buf.push(bytes.len() as u8);
            buf.extend_from_slice(bytes);
        }
    }
    let port = match endpoint {
        Endpoint::Ip(addr) => addr.port(),
        Endpoint::Domain(_, port) => *port,
    };
    buf.extend_from_slice(&port.to_be_bytes());
    buf
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn greeting_roundtrip(methods in prop::collection::vec(any::<u8>(), 1..=16)) {
        let buf = encode_greeting(&methods);
        let (parsed, consumed) = parse_greeting(&buf).unwrap();
        prop_assert_eq!(parsed, methods);
        prop_assert_eq!(consumed, buf.len());
    }

    #[test]
    fn auth_roundtrip(
        user in prop::string::string_regex("[a-z]{1,32}").unwrap(),
        pass in prop::string::string_regex("[a-z]{1,32}").unwrap(),
    ) {
        let buf = encode_auth(&user, &pass);
        let ((u, p), consumed) = parse_auth(&buf).unwrap();
        prop_assert_eq!(u, user);
        prop_assert_eq!(p, pass);
        prop_assert_eq!(consumed, buf.len());
    }

    #[test]
    fn request_ipv4_roundtrip(port in any::<u16>(), ip in any::<Ipv4Addr>()) {
        let ep = Endpoint::Ip(SocketAddr::new(IpAddr::V4(ip), port));
        let buf = encode_connect(&ep);
        let (parsed, consumed) = parse_request(&buf).unwrap();
        prop_assert_eq!(parsed.target, ep);
        prop_assert_eq!(consumed, buf.len());
    }

    #[test]
    fn request_ipv6_roundtrip(port in any::<u16>(), ip in any::<Ipv6Addr>()) {
        let ep = Endpoint::Ip(SocketAddr::new(IpAddr::V6(ip), port));
        let buf = encode_connect(&ep);
        let (parsed, consumed) = parse_request(&buf).unwrap();
        prop_assert_eq!(parsed.target, ep);
        prop_assert_eq!(consumed, buf.len());
    }

    #[test]
    fn request_domain_roundtrip(port in any::<u16>(), domain in valid_domain()) {
        let ep = Endpoint::Domain(domain.clone(), port);
        let buf = encode_connect(&ep);
        let (parsed, consumed) = parse_request(&buf).unwrap();
        prop_assert_eq!(parsed.target, ep);
        prop_assert_eq!(consumed, buf.len());
    }

    #[test]
    fn parse_greeting_never_panics(input in prop::collection::vec(any::<u8>(), 0..128)) {
        let _ = parse_greeting(&input);
    }

    #[test]
    fn parse_auth_never_panics(input in prop::collection::vec(any::<u8>(), 0..128)) {
        let _ = parse_auth(&input);
    }

    #[test]
    fn parse_request_never_panics(input in prop::collection::vec(any::<u8>(), 0..128)) {
        let _ = parse_request(&input);
    }

    #[test]
    fn greeting_incomplete_invariant(input in prop::collection::vec(any::<u8>(), 0..32)) {
        if let Err(ParseError::Incomplete { need, have }) = parse_greeting(&input) {
            assert_incomplete_invariant(have, need, input.len());
        }
    }

    #[test]
    fn greeting_ok_consumed_bounds(methods in prop::collection::vec(any::<u8>(), 1..=16)) {
        let buf = encode_greeting(&methods);
        let (_, consumed) = parse_greeting(&buf).unwrap();
        assert_consumed_in_bounds(consumed, buf.len());
    }
}
