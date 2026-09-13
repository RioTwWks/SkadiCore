//! XHTTP (SplitHTTP) inbound transport — stream-one MVP.

mod config;
mod stream_one;

pub use config::{PaddingRange, XhttpConfig, XhttpMode};
pub use stream_one::{accept_stream_one, StreamOneIo, XhttpError};
