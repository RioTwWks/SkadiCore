//! XHTTP (SplitHTTP) transport — inbound accept и client outbound.

mod accept;
mod client;
mod common;
mod config;
mod session;

pub use accept::{accept_stream_one, accept_xhttp, XhttpAcceptResult, XhttpError, XhttpIo};
pub use client::{
    connect_packet_up, connect_stream_one, connect_stream_up, connect_xhttp, XhttpClientConfig,
    XhttpClientError, XhttpClientIo,
};
pub use config::{PaddingRange, XhttpConfig, XhttpMode};
pub use session::XhttpSessionManager;
