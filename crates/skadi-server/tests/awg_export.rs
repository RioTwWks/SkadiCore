mod common;

use std::fs;
use tempfile::TempDir;

#[test]
fn export_awg_client_conf() {
    let keys = common::awg_fixtures::AwgTomlKeys::generate();
    let (_, client_private) = skadi_transport::generate_keypair();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("server.toml");
    fs::write(&path, common::awg_fixtures::minimal_awg_toml(&keys)).unwrap();
    let config = skadi_server::config::Config::load(&path).unwrap();
    let conf = config
        .export_awg_client_conf(0, &client_private, "vpn.example.com:51820")
        .expect("export");
    assert!(conf.contains("Endpoint = vpn.example.com:51820"));
    assert!(conf.contains("AllowedIPs = 0.0.0.0/0"));
}
