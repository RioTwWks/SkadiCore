//! Property-based тесты VLESS-парсеров.

mod proptest_common;

use proptest::prelude::*;
use proptest_common::{assert_consumed_in_bounds, assert_incomplete_invariant, valid_domain};
use skadi_core::Endpoint;
use skadi_protocol::vless::{
    build_addons_with_flow, build_mux_request, build_tcp_domain_request, build_tcp_request,
    build_udp_domain_request, build_udp_request, encode_port_address, parse_addons,
    parse_port_address, parse_request, CMD_MUX, CMD_TCP, CMD_UDP, MUX_PLACEHOLDER_HOST, ParseError,
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn port_address_ipv4_roundtrip(port in any::<u16>(), ip in any::<Ipv4Addr>()) {
        let ep = Endpoint::Ip(SocketAddr::new(IpAddr::V4(ip), port));
        let buf = encode_port_address(&ep);
        let (parsed, consumed) = parse_port_address(&buf).unwrap();
        prop_assert_eq!(parsed, ep);
        prop_assert_eq!(consumed, buf.len());
    }

    #[test]
    fn port_address_ipv6_roundtrip(port in any::<u16>(), ip in any::<Ipv6Addr>()) {
        let ep = Endpoint::Ip(SocketAddr::new(IpAddr::V6(ip), port));
        let buf = encode_port_address(&ep);
        let (parsed, consumed) = parse_port_address(&buf).unwrap();
        prop_assert_eq!(parsed, ep);
        prop_assert_eq!(consumed, buf.len());
    }

    #[test]
    fn port_address_domain_roundtrip(port in any::<u16>(), domain in valid_domain()) {
        let ep = Endpoint::Domain(domain.clone(), port);
        let buf = encode_port_address(&ep);
        let (parsed, consumed) = parse_port_address(&buf).unwrap();
        prop_assert_eq!(parsed, ep);
        prop_assert_eq!(consumed, buf.len());
    }

    #[test]
    fn tcp_request_ipv4_roundtrip(uuid in any::<[u8; 16]>(), port in any::<u16>(), ip in any::<Ipv4Addr>()) {
        let buf = build_tcp_request(&uuid, ip, port);
        let (req, consumed) = parse_request(&buf).unwrap();
        prop_assert_eq!(consumed, buf.len());
        prop_assert_eq!(req.uuid, uuid);
        prop_assert_eq!(req.command, CMD_TCP);
        prop_assert_eq!(req.target, Endpoint::Ip(SocketAddr::new(IpAddr::V4(ip), port)));
    }

    #[test]
    fn udp_request_ipv4_roundtrip(uuid in any::<[u8; 16]>(), port in any::<u16>(), ip in any::<Ipv4Addr>()) {
        let buf = build_udp_request(&uuid, ip, port);
        let (req, consumed) = parse_request(&buf).unwrap();
        prop_assert_eq!(consumed, buf.len());
        prop_assert_eq!(req.uuid, uuid);
        prop_assert_eq!(req.command, CMD_UDP);
    }

    #[test]
    fn tcp_domain_request_roundtrip(
        uuid in any::<[u8; 16]>(),
        port in any::<u16>(),
        domain in valid_domain(),
    ) {
        let buf = build_tcp_domain_request(&uuid, &domain, port);
        let (req, consumed) = parse_request(&buf).unwrap();
        prop_assert_eq!(consumed, buf.len());
        prop_assert_eq!(req.command, CMD_TCP);
        prop_assert_eq!(req.target, Endpoint::Domain(domain, port));
    }

    #[test]
    fn udp_domain_request_roundtrip(
        uuid in any::<[u8; 16]>(),
        port in any::<u16>(),
        domain in valid_domain(),
    ) {
        let buf = build_udp_domain_request(&uuid, &domain, port);
        let (req, consumed) = parse_request(&buf).unwrap();
        prop_assert_eq!(consumed, buf.len());
        prop_assert_eq!(req.command, CMD_UDP);
        prop_assert_eq!(req.target, Endpoint::Domain(domain, port));
    }

    #[test]
    fn mux_request_roundtrip(uuid in any::<[u8; 16]>()) {
        let buf = build_mux_request(&uuid);
        let (req, consumed) = parse_request(&buf).unwrap();
        prop_assert_eq!(consumed, buf.len());
        prop_assert_eq!(req.uuid, uuid);
        prop_assert_eq!(req.command, CMD_MUX);
        prop_assert_eq!(
            req.target,
            Endpoint::Domain(MUX_PLACEHOLDER_HOST.to_string(), 0)
        );
    }

    #[test]
    fn addons_flow_roundtrip(flow in prop::string::string_regex("[a-z-]{0,64}").unwrap()) {
        let buf = build_addons_with_flow(&flow);
        let addons = parse_addons(&buf).unwrap();
        prop_assert_eq!(addons.flow.as_deref(), Some(flow.as_str()));
    }

    #[test]
    fn parse_port_address_never_panics(input in prop::collection::vec(any::<u8>(), 0..128)) {
        let _ = parse_port_address(&input);
    }

    #[test]
    fn parse_request_never_panics(input in prop::collection::vec(any::<u8>(), 0..128)) {
        let _ = parse_request(&input);
    }

    #[test]
    fn parse_addons_never_panics(input in prop::collection::vec(any::<u8>(), 0..128)) {
        let _ = parse_addons(&input);
    }

    #[test]
    fn port_address_incomplete_invariant(input in prop::collection::vec(any::<u8>(), 0..32)) {
        if let Err(ParseError::Incomplete { need, have }) = parse_port_address(&input) {
            assert_incomplete_invariant(have, need, input.len());
        }
    }

    #[test]
    fn port_address_ok_consumed_bounds(port in any::<u16>(), ip in any::<Ipv4Addr>()) {
        let ep = Endpoint::Ip(SocketAddr::new(IpAddr::V4(ip), port));
        let buf = encode_port_address(&ep);
        let (_, consumed) = parse_port_address(&buf).unwrap();
        assert_consumed_in_bounds(consumed, buf.len());
    }
}
