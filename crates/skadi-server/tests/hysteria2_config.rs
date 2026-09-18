use std::fs;
use tempfile::TempDir;

const HY2_MINIMAL: &str = r#"
[server]
listen = "127.0.0.1:18080"

[protocol.socks5]
enabled = false

[protocol.vless]
enabled = false

[transport.hysteria2]
enabled = true
listen = ":8443"
password = "test-pass"
cert = "/tmp/cert.pem"
key = "/tmp/key.pem"
"#;

#[test]
fn hysteria2_config_validates() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("hy2.toml");
    fs::write(&path, HY2_MINIMAL).unwrap();
    skadi_server::check_config(&path).expect("valid hysteria2 config");
}
