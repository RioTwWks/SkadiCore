//! Клиентский режим SkadiCore: локальный SOCKS5 / TUN → удалённый VLESS+TLS.

mod config;
mod dns;
mod doh;
mod dot;
mod outbound;
mod pmtud;
mod runner;
mod socks5;
mod warnings;

#[cfg(target_os = "linux")]
mod tun;

pub use config::{
    ClientConfig, ClientListenConfig, RemoteConfig, RemoteTlsConfig, TunConfig, TunDnsConfig,
    TunRoutingConfig,
};
pub use runner::run;
