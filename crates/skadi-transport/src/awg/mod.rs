//! AmneziaWG (обфусцированный WireGuard) — userspace backend через `amneziawg-go`.

mod config;
mod keys;
mod manager;
pub mod render;

pub use config::{AwgObfuscationConfig, AwgPeerConfig, AwgServerConfig};
pub use keys::generate_keypair;
pub use manager::{AwgError, AwgManager};
pub use render::render_server_conf;
