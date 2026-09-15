//! Property-based тесты VLESS Mux frame-парсеров.

use proptest::prelude::*;
use skadi_core::Endpoint;
use skadi_protocol::vless::{
    encode_data_frame, encode_end_frame, encode_meta, parse_frame, parse_meta_body, MuxMeta,
    NETWORK_TCP, NETWORK_UDP, OPTION_DATA, SESSION_STATUS_END, SESSION_STATUS_KEEP,
    SESSION_STATUS_NEW,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn end_frame_roundtrip(session_id in any::<u16>(), with_error in any::<bool>()) {
        let frame = encode_end_frame(session_id, with_error);
        let (parsed, consumed) = parse_frame(&frame).unwrap();
        prop_assert_eq!(consumed, frame.len());
        prop_assert_eq!(parsed.meta.session_id, session_id);
        prop_assert_eq!(parsed.meta.status, SESSION_STATUS_END);
        prop_assert!(parsed.payload.is_none());
    }

    #[test]
    fn new_tcp_meta_roundtrip(
        session_id in any::<u16>(),
        port in any::<u16>(),
        ip in any::<Ipv4Addr>(),
    ) {
        let target = Endpoint::Ip(SocketAddr::new(IpAddr::V4(ip), port));
        let meta = MuxMeta {
            session_id,
            status: SESSION_STATUS_NEW,
            option: OPTION_DATA,
            network: Some(NETWORK_TCP),
            target: Some(target.clone()),
            global_id: None,
        };
        let encoded = encode_meta(&meta);
        let parsed = parse_meta_body(&encoded[2..]).unwrap();
        prop_assert_eq!(parsed.session_id, session_id);
        prop_assert_eq!(parsed.status, SESSION_STATUS_NEW);
        prop_assert_eq!(parsed.target, Some(target));
    }

    #[test]
    fn data_frame_roundtrip(
        session_id in any::<u16>(),
        port in any::<u16>(),
        ip in any::<Ipv4Addr>(),
        payload in prop::collection::vec(any::<u8>(), 0..1024),
    ) {
        let target = Endpoint::Ip(SocketAddr::new(IpAddr::V4(ip), port));
        let meta = MuxMeta {
            session_id,
            status: SESSION_STATUS_NEW,
            option: OPTION_DATA,
            network: Some(NETWORK_TCP),
            target: Some(target),
            global_id: None,
        };
        let frame = encode_data_frame(&meta, &payload).unwrap();
        let (parsed, consumed) = parse_frame(&frame).unwrap();
        prop_assert_eq!(consumed, frame.len());
        prop_assert_eq!(parsed.payload.as_deref(), Some(payload.as_slice()));
    }

    #[test]
    fn keep_udp_meta_roundtrip(port in any::<u16>(), ip in any::<Ipv4Addr>()) {
        let target = Endpoint::Ip(SocketAddr::new(IpAddr::V4(ip), port));
        let meta = MuxMeta {
            session_id: 0,
            status: SESSION_STATUS_KEEP,
            option: OPTION_DATA,
            network: Some(NETWORK_UDP),
            target: Some(target.clone()),
            global_id: None,
        };
        let encoded = encode_meta(&meta);
        let parsed = parse_meta_body(&encoded[2..]).unwrap();
        prop_assert_eq!(parsed.status, SESSION_STATUS_KEEP);
        prop_assert_eq!(parsed.target, Some(target));
    }

    #[test]
    fn parse_frame_never_panics(input in prop::collection::vec(any::<u8>(), 0..256)) {
        let _ = parse_frame(&input);
    }
}
