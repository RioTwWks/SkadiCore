//! Рендер YAML-конфига для `hysteria server`.

use super::config::Hysteria2ServerConfig;
use anyhow::Result;

/// Собрать минимальный `config.yaml` для Hysteria 2 server.
pub fn render_server_yaml(config: &Hysteria2ServerConfig) -> Result<String> {
    let listen = if config.listen.ip().is_unspecified() {
        format!(":{}", config.listen.port())
    } else if config.listen.is_ipv6() {
        format!("[{}]:{}", config.listen.ip(), config.listen.port())
    } else {
        format!("{}:{}", config.listen.ip(), config.listen.port())
    };

    let mut yaml = format!(
        "listen: {}\n\ntls:\n  cert: {}\n  key: {}\n\nauth:\n  type: password\n  password: {}\n",
        listen, config.cert_path, config.key_path, config.password
    );

    if let Some(url) = &config.masquerade_url {
        if !url.trim().is_empty() {
            yaml.push_str("\nmasquerade:\n  type: proxy\n  proxy:\n");
            yaml.push_str(&format!("    url: {}\n", url.trim()));
            yaml.push_str("    rewriteHost: true\n");
        }
    }

    Ok(yaml)
}
