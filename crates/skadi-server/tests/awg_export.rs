use std::fs;
use tempfile::TempDir;

const AWG_EXPORT: &str = r#"
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
fn export_awg_client_conf() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("server.toml");
    fs::write(&path, AWG_EXPORT).unwrap();
    let config = skadi_server::config::Config::load(&path).unwrap();
    let conf = config
        .export_awg_client_conf(
            0,
            "wSf6Og3dFsu4+nhBpvSwjXiByo7Q8e8KKis/SDDvlEw=",
            "vpn.example.com:51820",
        )
        .expect("export");
    assert!(conf.contains("Endpoint = vpn.example.com:51820"));
    assert!(conf.contains("AllowedIPs = 0.0.0.0/0"));
}
