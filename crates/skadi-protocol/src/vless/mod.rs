//! VLESS protocol (version 0).
//!
//! Аутентификация по UUID, компактный бинарный заголовок,
//! безопасность обеспечивается транспортным слоем (TLS/REALITY).

pub mod addons;
pub mod client;
pub mod config;
pub mod handler;
pub mod mux;
pub mod parse;

pub use addons::{build_addons_with_flow, parse_addons, VlessAddons, FLOW_XTLS_VISION};
pub use client::{build_tcp_endpoint_request, VlessClient};
pub use config::{Uuid, UuidParseError, VlessConfig, VlessUser};
pub use handler::{VlessHandler, VlessHandshake};
pub use mux::{
    encode_data_frame, encode_end_frame, encode_meta, parse_frame, parse_meta_body, MuxError,
    MuxFrame, MuxMeta, GLOBAL_ID_LEN, NETWORK_TCP, NETWORK_UDP, OPTION_DATA, SESSION_STATUS_END,
    SESSION_STATUS_KEEP, SESSION_STATUS_KEEP_ALIVE, SESSION_STATUS_NEW, XUDP_SESSION_ID,
};
pub use parse::{
    build_mux_request, build_response_header, build_tcp_domain_request, build_tcp_request,
    build_udp_domain_request, build_udp_request, encode_port_address, parse_port_address,
    parse_request, ParseError, VlessRequest, ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6, CMD_MUX, CMD_TCP,
    CMD_UDP, MUX_PLACEHOLDER_HOST, VLESS_VERSION,
};
