//! Общие фикстуры для интеграционных тестов skadi-transport.

use skadi_transport::{
    generate_keypair, AwgClientConfig, AwgObfuscationConfig, AwgPeerConfig, AwgServerConfig,
};

pub const TEST_ONLY_PASSWORD: &str = "test-only-not-a-secret";
pub const TEST_ONLY_UUID: &str = "00000000-0000-0000-0000-000000000001";

/// Пара ключей для AWG-тестов.
pub struct AwgTestKeys {
    pub server_private: String,
    pub peer_public: String,
    pub client_private: String,
    pub server_public: String,
}

impl AwgTestKeys {
    pub fn generate() -> Self {
        let (server_private, server_public) = generate_keypair();
        let (client_private, peer_public) = generate_keypair();
        Self {
            server_private,
            peer_public,
            client_private,
            server_public,
        }
    }
}

pub fn sample_obfuscation() -> AwgObfuscationConfig {
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

pub fn sample_awg_server_config(iface: &str, keys: &AwgTestKeys) -> AwgServerConfig {
    AwgServerConfig {
        listen: "127.0.0.1:51820".parse().unwrap(),
        interface_name: iface.into(),
        private_key: keys.server_private.clone(),
        address: "10.8.0.1/24".into(),
        mtu: Some(1420),
        obfuscation: sample_obfuscation(),
        peers: vec![AwgPeerConfig {
            public_key: keys.peer_public.clone(),
            allowed_ips: vec!["10.8.0.2/32".into()],
            preshared_key: None,
            endpoint: None,
            persistent_keepalive: Some(25),
        }],
    }
}

pub fn sample_awg_client_config(iface: &str, keys: &AwgTestKeys) -> AwgClientConfig {
    AwgClientConfig {
        interface_name: iface.into(),
        private_key: keys.client_private.clone(),
        address: "10.8.0.2/24".into(),
        server_public_key: keys.server_public.clone(),
        endpoint: "127.0.0.1:51820".into(),
        mtu: Some(1420),
        dns: Some("1.1.1.1".into()),
        allowed_ips: vec!["0.0.0.0/0".into()],
        persistent_keepalive: Some(25),
        obfuscation: sample_obfuscation(),
    }
}
