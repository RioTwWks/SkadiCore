//! Тестовые AWG-конфиги с ключами, сгенерированными на лету.

use skadi_transport::generate_keypair;

/// Пара ключей для TOML-фикстур AWG.
pub struct AwgTomlKeys {
    pub server_private: String,
    pub peer_public: String,
}

impl AwgTomlKeys {
    pub fn generate() -> Self {
        let (server_private, _) = generate_keypair();
        let (_, peer_public) = generate_keypair();
        Self {
            server_private,
            peer_public,
        }
    }
}

pub fn minimal_awg_toml(keys: &AwgTomlKeys) -> String {
    format!(
        r#"
[server]
listen = "127.0.0.1:18080"

[protocol.socks5]
enabled = false

[protocol.vless]
enabled = false

[transport.awg]
enabled = true
listen = "127.0.0.1:51820"
private_key = "{server_private}"
address = "10.8.0.1/24"

[[transport.awg.peers]]
public_key = "{peer_public}"
allowed_ips = ["10.8.0.2/32"]
"#,
        server_private = keys.server_private,
        peer_public = keys.peer_public,
    )
}

pub fn awg_toml_with_nat(keys: &AwgTomlKeys) -> String {
    format!(
        r#"
[server]
listen = "127.0.0.1:18080"

[protocol.socks5]
enabled = false

[protocol.vless]
enabled = false

[transport.awg]
enabled = true
listen = "127.0.0.1:51820"
private_key = "{server_private}"
address = "10.8.0.1/24"

[transport.awg.nat]
enabled = true
egress_interface = "eth0"
subnet = "10.8.0.0/24"

[[transport.awg.peers]]
public_key = "{peer_public}"
allowed_ips = ["10.8.0.2/32"]
"#,
        server_private = keys.server_private,
        peer_public = keys.peer_public,
    )
}

pub fn awg_toml_no_peers(server_private: &str) -> String {
    format!(
        r#"
[server]
listen = "127.0.0.1:18080"
[protocol.vless]
enabled = false
[protocol.socks5]
enabled = false
[transport.awg]
enabled = true
private_key = "{server_private}"
"#,
        server_private = server_private,
    )
}
