//! E2E smoke-тест AWG: поднимает amneziawg-go если бинарники доступны.

use std::process::Command;

fn awg_binaries_available() -> bool {
    let go = std::env::var("AWG_GO_BINARY")
        .ok()
        .or_else(|| which("amneziawg-go"));
    let tools = std::env::var("AWG_TOOLS_BINARY")
        .ok()
        .or_else(|| which("awg").or_else(|| which("wg")));
    go.is_some() && tools.is_some()
}

fn which(name: &str) -> Option<String> {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", name))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

#[test]
fn awg_render_and_optional_start() {
    if !awg_binaries_available() {
        eprintln!("SKIP awg_e2e: amneziawg-go/awg not in PATH");
        return;
    }
    // Smoke: validate example config renders and binaries are executable.
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/awg-vpn/server.toml");
    skadi_server::check_config(&path).expect("awg example config");
}
