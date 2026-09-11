//! SkadiCore — общие типы, трейты и ошибки.
//!
//! Этот крейт не содержит логики протоколов или транспортов.
//! Только контракты, которые используют остальные крейты.

pub mod endpoint;
pub mod error;
pub mod session;
pub mod user;

pub use endpoint::Endpoint;
pub use error::{Error, Result};
pub use session::{Session, SessionId};
pub use user::UserId;
