//! XHTTP (SplitHTTP) inbound transport — stream-one и stream-up.

mod accept;
mod common;
mod config;
mod session;

pub use accept::{accept_stream_one, accept_xhttp, XhttpAcceptResult, XhttpError, XhttpIo};
pub use config::{PaddingRange, XhttpConfig, XhttpMode};
pub use session::XhttpSessionManager;
