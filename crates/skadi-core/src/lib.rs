//! SkadiCore — общие типы и ошибки.
//!
//! Без протоколов, транспортов и async runtime (`tokio` / `tracing` не зависят).
//! Опционально: feature `serde` (в `default`) для десериализации конфигов.

pub mod endpoint;
pub mod error;
pub mod protocol;
pub mod secret;
pub mod session;
pub mod user;

pub use endpoint::{is_forbidden_ip, validate_outbound_literal, validate_resolved_addrs, Endpoint};
pub use error::{Error, Result};
pub use protocol::{EnabledProtocols, Protocol, SOCKS5_WIRE_BYTE, VLESS_WIRE_BYTE};
pub use secret::{redact_bearer_tokens, SecretString, REDACTED};
pub use session::{Session, SessionId};
pub use user::UserId;
