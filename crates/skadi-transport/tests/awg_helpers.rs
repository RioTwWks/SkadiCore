//! Unit-тесты AWG helpers: keys, NAT, config paths.

use skadi_transport::{
    apply_nat, generate_keypair, public_key_from_private, AwgClientConfig, AwgNatConfig,
    AwgObfuscationConfig, AwgServerConfig,
};

#[test]
fn generate_and_derive_keypair_roundtrip() {
    let (private, public) = generate_keypair();
    let derived = public_key_from_private(&private).expect("derive public key");
    assert_eq!(derived, public);
}

#[test]
fn public_key_from_private_rejects_invalid() {
    let err = public_key_from_private("not-a-key").unwrap_err();
    assert!(err.contains("invalid AWG private_key"));
}

#[test]
fn awg_uapi_socket_paths() {
    let prev = std::env::var("AWG_UAPI_DIR").ok();
    std::env::remove_var("AWG_UAPI_DIR");

    let (server_private, server_public) = generate_keypair();
    let (client_private, _) = generate_keypair();

    let server = AwgServerConfig {
        listen: "0.0.0.0:51820".parse().unwrap(),
        interface_name: "skadiwg0".into(),
        private_key: server_private,
        address: "10.8.0.1/24".into(),
        mtu: None,
        obfuscation: sample_obfuscation(),
        peers: vec![],
    };
    assert_eq!(
        server.uapi_socket_path(),
        "/var/run/amneziawg/skadiwg0.sock"
    );

    let client = AwgClientConfig {
        interface_name: "skadiawg0".into(),
        private_key: client_private,
        address: "10.8.0.2/24".into(),
        server_public_key: server_public,
        endpoint: "127.0.0.1:51820".into(),
        mtu: None,
        dns: None,
        allowed_ips: vec!["0.0.0.0/0".into()],
        persistent_keepalive: None,
        obfuscation: sample_obfuscation(),
    };
    assert_eq!(
        client.uapi_socket_path(),
        "/var/run/amneziawg/skadiawg0.sock"
    );

    std::env::set_var("AWG_UAPI_DIR", "/tmp/custom-awg");
    let override_server = AwgServerConfig {
        listen: server.listen,
        interface_name: "skadiwg1".into(),
        private_key: server.private_key.clone(),
        address: server.address.clone(),
        mtu: None,
        obfuscation: sample_obfuscation(),
        peers: vec![],
    };
    assert_eq!(
        override_server.uapi_socket_path(),
        "/tmp/custom-awg/skadiwg1.sock"
    );

    match prev {
        Some(v) => std::env::set_var("AWG_UAPI_DIR", v),
        None => std::env::remove_var("AWG_UAPI_DIR"),
    }
}

#[test]
fn apply_nat_disabled_is_noop() {
    let config = AwgNatConfig {
        enabled: false,
        egress_interface: None,
        subnet: "10.8.0.0/24".into(),
    };
    apply_nat(&config).expect("disabled NAT should succeed");
}

#[test]
fn apply_nat_rejects_empty_subnet() {
    let config = AwgNatConfig {
        enabled: true,
        egress_interface: Some("eth0".into()),
        subnet: "  ".into(),
    };
    let err = apply_nat(&config).unwrap_err();
    assert!(err.to_string().contains("NAT subnet"));
}

fn sample_obfuscation() -> AwgObfuscationConfig {
    AwgObfuscationConfig {
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
    }
}
