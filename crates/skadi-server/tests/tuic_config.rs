use std::fs;
use tempfile::TempDir;

const TUIC_MINIMAL: &str = r#"
[server]
listen = "127.0.0.1:18080"

[protocol.socks5]
enabled = false

[protocol.vless]
enabled = false

[transport.tuic]
enabled = true
listen = "127.0.0.1:8443"
uuid = "00000000-0000-0000-0000-000000000001"
password = "test-only-not-a-secret"
certificate = "/tmp/cert.pem"
private_key = "/tmp/key.pem"
"#;

#[test]
fn tuic_config_validates() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tuic.toml");
    fs::write(&path, TUIC_MINIMAL).unwrap();
    skadi_server::check_config(&path).expect("valid tuic config");
}
