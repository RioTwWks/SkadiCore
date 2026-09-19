//! Unit-тесты рендера клиентского AWG `.conf`.

mod common;

use skadi_transport::{render_client_conf, AwgClientExport, AwgServerConfig};

#[test]
fn render_awg_client_conf_contains_peer_fields() {
    let keys = common::AwgTestKeys::generate();
    let obf = common::sample_obfuscation();
    let server = AwgServerConfig {
        listen: "0.0.0.0:51820".parse().unwrap(),
        interface_name: "skadiwg0".into(),
        private_key: keys.server_private,
        address: "10.8.0.1/24".into(),
        mtu: Some(1420),
        obfuscation: obf.clone(),
        peers: vec![],
    };
    let client = AwgClientExport {
        private_key: keys.client_private,
        address: "10.8.0.2/32".into(),
        dns: Some("1.1.1.1".into()),
        allowed_ips: vec!["0.0.0.0/0".into()],
        persistent_keepalive: Some(25),
    };
    let conf = render_client_conf(
        &keys.server_public,
        "127.0.0.1:51820",
        &client,
        &obf,
        server.mtu,
    )
    .expect("render client");
    assert!(conf.contains("PrivateKey"));
    assert!(conf.contains("PublicKey"));
    assert!(conf.contains("Endpoint = 127.0.0.1:51820"));
    assert!(conf.contains("AllowedIPs = 0.0.0.0/0"));
    assert!(conf.contains("Jc = 8"));
}
