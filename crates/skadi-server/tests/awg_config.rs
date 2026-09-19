//! Валидация `[transport.awg]` в server config.

mod common;

use std::fs;
use tempfile::TempDir;

#[test]
fn awg_config_validates() {
    let keys = common::awg_fixtures::AwgTomlKeys::generate();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("awg.toml");
    fs::write(&path, common::awg_fixtures::minimal_awg_toml(&keys)).unwrap();
    skadi_server::check_config(&path).expect("valid awg config");
}

#[test]
fn awg_example_placeholders_pass_check_config() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/awg-vpn/server.toml");
    skadi_server::check_config(&path).expect("awg-vpn example with placeholder keys");
}

#[test]
fn awg_config_requires_peer() {
    let (server_private, _) = skadi_transport::generate_keypair();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("awg-no-peers.toml");
    fs::write(
        &path,
        common::awg_fixtures::awg_toml_no_peers(&server_private),
    )
    .unwrap();
    let err = skadi_server::check_config(&path).unwrap_err();
    assert!(
        err.to_string().contains("peers"),
        "unexpected error: {}",
        err
    );
}
