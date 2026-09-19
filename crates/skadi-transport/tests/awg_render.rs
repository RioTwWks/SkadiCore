//! Unit-тесты рендера AmneziaWG конфигурации.

mod common;

use skadi_transport::{render_server_conf, AwgPeerConfig, AwgServerConfig};

#[test]
fn render_awg_server_conf_contains_obfuscation() {
    let keys = common::AwgTestKeys::generate();
    let config = AwgServerConfig {
        listen: "0.0.0.0:51820".parse().unwrap(),
        interface_name: "skadiwg0".into(),
        private_key: keys.server_private,
        address: "10.8.0.1/24".into(),
        mtu: Some(1420),
        obfuscation: common::sample_obfuscation(),
        peers: vec![AwgPeerConfig {
            public_key: keys.peer_public,
            allowed_ips: vec!["10.8.0.2/32".into()],
            preshared_key: None,
            endpoint: None,
            persistent_keepalive: Some(25),
        }],
    };

    let conf = render_server_conf(&config).expect("render");
    assert!(conf.contains("PrivateKey"));
    assert!(conf.contains("Jc = 8"));
    assert!(conf.contains("H1 = 1-10000000"));
    assert!(conf.contains("AllowedIPs = 10.8.0.2/32"));
}
