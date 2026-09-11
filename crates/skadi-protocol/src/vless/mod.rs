//! VLESS protocol (version 0).
//!
//! Аутентификация по UUID, компактный бинарный заголовок,
//! безопасность обеспечивается транспортным слоем (TLS/REALITY).

pub mod config;
pub mod handler;
pub mod parse;

pub use config::{Uuid, UuidParseError, VlessConfig, VlessUser};
pub use handler::VlessHandler;
pub use parse::{
    build_response_header, parse_request, ParseError, VlessRequest,
    ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6, CMD_MUX, CMD_TCP, CMD_UDP,
    VLESS_VERSION,
};
