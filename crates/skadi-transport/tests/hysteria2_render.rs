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
