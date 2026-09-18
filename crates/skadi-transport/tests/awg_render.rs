//! Unit-тесты рендера AmneziaWG конфигурации.

use skadi_transport::{render_server_conf, AwgObfuscationConfig, AwgPeerConfig, AwgServerConfig};

#[test]
fn render_awg_server_conf_contains_obfuscation() {
    let config = AwgServerConfig {
        listen: "0.0.0.0:51820".parse().unwrap(),
        interface_name: "skadiwg0".into(),
        private_key: "sJkP2oorqrq49P6Ln25MWo3X04PxhB8k+RnJJnZ4gEo=".into(),
        address: "10.8.0.1/24".into(),
        mtu: Some(1420),
        obfuscation: AwgObfuscationConfig {
            jc: 8,
            jmin: 64,
            jmax: 1024,
            s1: 32,
            s2: 32,
            s3: 16,
            s4: 16,
            h1: "1-10000000".into(),
            h2: "10000001-20000000".into(),
            h3: "20000001-30000000".into(),
            h4: "30000001-40000000".into(),
        },
        peers: vec![AwgPeerConfig {
            public_key: "kHkjzj1KeQjR/82vXYRdQPA113MAzNRkDsedH5kZLi4=".into(),
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
