//! SkadiCore — общие типы, трейты и ошибки.
//!
//! Этот крейт не содержит логики протоколов или транспортов.
//! Только контракты, которые используют остальные крейты.

pub mod endpoint;
pub mod error;
pub mod protocol;
pub mod session;
pub mod user;

pub use endpoint::{is_forbidden_ip, validate_outbound_literal, validate_resolved_addrs, Endpoint};
pub use error::{Error, Result};
pub use protocol::{EnabledProtocols, Protocol, SOCKS5_WIRE_BYTE, VLESS_WIRE_BYTE};
pub use session::{Session, SessionId};
pub use user::UserId;
