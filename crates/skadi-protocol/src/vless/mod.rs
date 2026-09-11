//! VLESS protocol (version 0).
//!
//! Аутентификация по UUID, компактный бинарный заголовок,
//! безопасность обеспечивается транспортным слоем (TLS/REALITY).

pub mod addons;
pub mod config;
pub mod handler;
pub mod parse;

pub use addons::{build_addons_with_flow, parse_addons, VlessAddons, FLOW_XTLS_VISION};
pub use config::{Uuid, UuidParseError, VlessConfig, VlessUser};
pub use handler::{VlessHandler, VlessHandshake};
pub use parse::{
    build_response_header, build_tcp_domain_request, build_tcp_request, build_udp_domain_request,
    build_udp_request, parse_request, ParseError, VlessRequest, ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6,
    CMD_MUX, CMD_TCP, CMD_UDP, VLESS_VERSION,
};
