//! CLI: `skadicore check-config`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn workspace_config() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../config/skadi.toml")
        .canonicalize()
        .expect("config/skadi.toml")
}

const VALID_MINIMAL: &str = r#"
[server]
listen = "127.0.0.1:18080"

[protocol.vless]
enabled = true

[[protocol.vless.users]]
id = "b831381d-6324-4d53-ad4f-8cda48b30811"
"#;

#[test]
fn check_config_valid_exits_zero() {
    let config = workspace_config();
    let output = Command::new(env!("CARGO_BIN_EXE_skadicore"))
        .args(["check-config", "--config", config.to_str().unwrap()])
        .output()
        .expect("spawn skadicore");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Configuration OK"));
    assert!(stdout.contains("listen:"));
}

#[test]
fn check_config_invalid_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.toml");
    fs::write(&path, "[server]\nlisten = \"not-a-socket\"\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_skadicore"))
        .args(["check-config", "--config", path.to_str().unwrap()])
        .output()
        .expect("spawn skadicore");

    assert!(!output.status.success());
}

#[test]
fn check_config_lib_helper() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ok.toml");
    fs::write(&path, VALID_MINIMAL).unwrap();
    skadi_server::check_config(&path).expect("valid config");
}

#[test]
fn check_config_awg_example() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/awg-vpn/server.toml");
    skadi_server::check_config(&path).expect("awg-vpn example config");
}

#[test]
fn check_config_reality_xhttp_example() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/reality-xhttp-vless/server.toml");
    skadi_server::check_config(&path).expect("reality-xhttp example config");
}
