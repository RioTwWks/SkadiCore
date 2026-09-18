//! Валидация `[transport.awg]` в server config.

use std::fs;
use tempfile::TempDir;

const AWG_MINIMAL: &str = r#"
[server]
listen = "127.0.0.1:18080"

[protocol.socks5]
enabled = false

[protocol.vless]
enabled = false

[transport.awg]
enabled = true
listen = "127.0.0.1:51820"
private_key = "sJkP2oorqrq49P6Ln25MWo3X04PxhB8k+RnJJnZ4gEo="
address = "10.8.0.1/24"

[[transport.awg.peers]]
public_key = "kHkjzj1KeQjR/82vXYRdQPA113MAzNRkDsedH5kZLi4="
allowed_ips = ["10.8.0.2/32"]
"#;

#[test]
fn awg_config_validates() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("awg.toml");
    fs::write(&path, AWG_MINIMAL).unwrap();
    skadi_server::check_config(&path).expect("valid awg config");
}

#[test]
fn awg_config_requires_peer() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("awg-no-peers.toml");
    fs::write(
        &path,
        r#"
[server]
listen = "127.0.0.1:18080"
[protocol.vless]
enabled = false
[protocol.socks5]
enabled = false
[transport.awg]
enabled = true
private_key = "sJkP2oorqrq49P6Ln25MWo3X04PxhB8k+RnJJnZ4gEo="
"#,
    )
    .unwrap();
    let err = skadi_server::check_config(&path).unwrap_err();
    assert!(
        err.to_string().contains("peers"),
        "unexpected error: {}",
        err
    );
}
