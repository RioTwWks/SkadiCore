use skadi_transport::{render_hysteria2_yaml, Hysteria2ServerConfig};

#[test]
fn render_hysteria2_yaml_contains_auth() {
    let config = Hysteria2ServerConfig {
        listen: "0.0.0.0:443".parse().unwrap(),
        password: "test-secret".into(),
        cert_path: "/tmp/cert.pem".into(),
        key_path: "/tmp/key.pem".into(),
        masquerade_url: Some("https://www.bing.com".into()),
    };
    let yaml = render_hysteria2_yaml(&config).expect("render");
    assert!(yaml.contains("password: test-secret"));
    assert!(yaml.contains("cert: /tmp/cert.pem"));
    assert!(yaml.contains("masquerade:"));
}

#[test]
fn render_hysteria2_yaml_ipv6_and_unspecified_listen() {
    let ipv6 = Hysteria2ServerConfig {
        listen: "[::1]:8443".parse().unwrap(),
        password: "pw".into(),
        cert_path: "/tmp/cert.pem".into(),
        key_path: "/tmp/key.pem".into(),
        masquerade_url: None,
    };
    let yaml_v6 = render_hysteria2_yaml(&ipv6).expect("render v6");
    assert!(yaml_v6.contains("listen: [::1]:8443"));

    let unspec = Hysteria2ServerConfig {
        listen: "0.0.0.0:9443".parse().unwrap(),
        password: "pw".into(),
        cert_path: "/tmp/cert.pem".into(),
        key_path: "/tmp/key.pem".into(),
        masquerade_url: Some("  ".into()),
    };
    let yaml_unspec = render_hysteria2_yaml(&unspec).expect("render unspec");
    assert!(yaml_unspec.contains("listen: :9443"));
    assert!(!yaml_unspec.contains("masquerade:"));
}
