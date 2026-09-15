//! Клиентский режим SkadiCore: локальный SOCKS5 / TUN → удалённый VLESS+TLS.

mod config;
mod outbound;
mod runner;
mod socks5;

#[cfg(target_os = "linux")]
mod tun;

pub use config::{ClientConfig, ClientListenConfig, RemoteConfig, RemoteTlsConfig, TunConfig};
pub use runner::run;
