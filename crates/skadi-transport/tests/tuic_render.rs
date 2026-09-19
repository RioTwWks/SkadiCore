mod common;

use skadi_transport::{render_tuic_toml, TuicServerConfig};

#[test]
fn render_tuic_toml_contains_user() {
    let config = TuicServerConfig {
        listen: "0.0.0.0:8443".parse().unwrap(),
        uuid: common::TEST_ONLY_UUID.into(),
        password: common::TEST_ONLY_PASSWORD.into(),
        cert_path: "/tmp/cert.pem".into(),
        key_path: "/tmp/key.pem".into(),
        congestion_control: "bbr".into(),
        alpn: vec!["h3".into()],
    };
    let toml = render_tuic_toml(&config).expect("render");
    assert!(toml.contains(common::TEST_ONLY_UUID));
    assert!(toml.contains(common::TEST_ONLY_PASSWORD));
}
