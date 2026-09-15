//! Клиентский режим SkadiCore: локальный SOCKS5 → удалённый VLESS+TLS.

mod config;
mod runner;

pub use config::{ClientConfig, ClientListenConfig, RemoteConfig, RemoteTlsConfig};
pub use runner::run;
