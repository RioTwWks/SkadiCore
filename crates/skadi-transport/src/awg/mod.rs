//! AmneziaWG (обфусцированный WireGuard) — userspace backend через `amneziawg-go`.

mod config;
mod keys;
mod manager;
mod nat;
pub mod render;

pub use config::{AwgClientConfig, AwgObfuscationConfig, AwgPeerConfig, AwgServerConfig};
pub use keys::{generate_keypair, public_key_from_private};
pub use manager::{AwgClientManager, AwgError, AwgManager};
pub use nat::{apply_nat, AwgNatConfig};
pub use render::{render_client_conf, render_server_conf, AwgClientExport};
