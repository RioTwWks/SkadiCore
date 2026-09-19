//! Протоколы: SOCKS5, VLESS, REALITY.

pub mod socks5;
pub mod vless;

pub use socks5::{
    AuthMethod, Socks5Config, Socks5Handler, Socks5Request, UserCredential, CMD_BIND,
    CMD_UDP_ASSOCIATE, REP_ADDRESS_NOT_SUPPORTED, REP_COMMAND_NOT_SUPPORTED,
    REP_CONNECTION_REFUSED, REP_GENERAL_FAILURE, REP_HOST_UNREACHABLE, REP_NETWORK_UNREACHABLE,
    REP_NOT_ALLOWED, REP_SUCCEEDED,
};
pub use vless::{VlessConfig, VlessHandler, VlessHandshake, VlessUser, CMD_MUX, CMD_TCP, CMD_UDP};
