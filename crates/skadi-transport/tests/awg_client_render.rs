//! Unit-тесты рендера клиентского AWG `.conf`.

use skadi_transport::{render_client_conf, AwgClientExport, AwgObfuscationConfig, AwgServerConfig};

#[test]
fn render_awg_client_conf_contains_peer_fields() {
    let obf = AwgObfuscationConfig {
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
    };
    let server = AwgServerConfig {
        listen: "0.0.0.0:51820".parse().unwrap(),
        interface_name: "skadiwg0".into(),
        private_key: "sJkP2oorqrq49P6Ln25MWo3X04PxhB8k+RnJJnZ4gEo=".into(),
        address: "10.8.0.1/24".into(),
        mtu: Some(1420),
        obfuscation: obf.clone(),
        peers: vec![],
    };
    let client = AwgClientExport {
        private_key: "kHkjzj1KeQjR/82vXYRdQPA113MAzNRkDsedH5kZLi4=".into(),
        address: "10.8.0.2/32".into(),
        dns: Some("1.1.1.1".into()),
        allowed_ips: vec!["0.0.0.0/0".into()],
        persistent_keepalive: Some(25),
    };
    let server_public = "BKVtmgSy1V3vWdvrZ8NWdxgPACG6OBVH3pH8ptdNZFA=";
    let conf = render_client_conf(server_public, "127.0.0.1:51820", &client, &obf, server.mtu)
        .expect("render client");
    assert!(conf.contains("PrivateKey"));
    assert!(conf.contains("PublicKey"));
    assert!(conf.contains("Endpoint = 127.0.0.1:51820"));
    assert!(conf.contains("AllowedIPs = 0.0.0.0/0"));
    assert!(conf.contains("Jc = 8"));
}
