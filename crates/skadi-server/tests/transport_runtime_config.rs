//! Runtime-конфиги AWG NAT / Hysteria2 / TUIC из TOML.

use skadi_server::config::Config;
use std::fs;
use tempfile::TempDir;

const AWG_WITH_NAT: &str = r#"
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

[transport.awg.nat]
enabled = true
egress_interface = "eth0"
subnet = "10.8.0.0/24"

[[transport.awg.peers]]
public_key = "kHkjzj1KeQjR/82vXYRdQPA113MAzNRkDsedH5kZLi4="
allowed_ips = ["10.8.0.2/32"]
"#;

const HY2_WITH_MASQUERADE: &str = r#"
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
masquerade_url = "https://example.com"
"#;

#[test]
fn awg_nat_runtime_config() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("awg-nat.toml");
    fs::write(&path, AWG_WITH_NAT).unwrap();
    let config = Config::load(&path).expect("load");
    let nat = config.awg_nat_config();
    assert!(nat.enabled);
    assert_eq!(nat.egress_interface.as_deref(), Some("eth0"));
    assert_eq!(nat.subnet, "10.8.0.0/24");

    let exported = config
        .export_awg_client_conf(
            0,
            "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=",
            "203.0.113.1:51820",
        )
        .expect("export");
    assert!(exported.contains("203.0.113.1:51820"));
    assert!(exported.contains("Endpoint"));
}

#[test]
fn hysteria2_runtime_config_with_masquerade() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("hy2.toml");
    fs::write(&path, HY2_WITH_MASQUERADE).unwrap();
    let config = Config::load(&path).expect("load");
    assert!(config.hysteria2_enabled());
    let runtime = config.hysteria2_server_config().expect("runtime");
    assert_eq!(runtime.listen.port(), 8443);
    assert_eq!(
        runtime.masquerade_url.as_deref(),
        Some("https://example.com")
    );
}

#[test]
fn tuic_runtime_config() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tuic.toml");
    fs::write(
        &path,
        r#"
[server]
listen = "127.0.0.1:18080"
[protocol.socks5]
enabled = false
[protocol.vless]
enabled = false
[transport.tuic]
enabled = true
listen = "127.0.0.1:8443"
uuid = "550e8400-e29b-41d4-a716-446655440000"
password = "secret"
certificate = "/tmp/cert.pem"
private_key = "/tmp/key.pem"
congestion_control = "cubic"
alpn = ["h3"]
"#,
    )
    .unwrap();
    let config = Config::load(&path).expect("load");
    assert!(config.tuic_enabled());
    let runtime = config.tuic_server_config().expect("runtime");
    assert_eq!(runtime.uuid, "550e8400-e29b-41d4-a716-446655440000");
    assert_eq!(runtime.alpn, vec!["h3".to_string()]);
}
