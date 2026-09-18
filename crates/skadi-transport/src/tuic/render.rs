//! Рендер TOML-конфига для `tuic-server`.

use super::config::TuicServerConfig;
use anyhow::Result;

/// Собрать минимальный `config.toml` для tuic-server.
pub fn render_server_toml(config: &TuicServerConfig) -> Result<String> {
    let alpn = config
        .alpn
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<_>>()
        .join(", ");

    let toml = format!(
        r#"[server]
listen = "{listen}"
users = [
    {{ uuid = "{uuid}", password = "{password}" }}
]

[server.tls]
certificate = "{cert}"
private_key = "{key}"

[server.quic]
congestion_control = "{cc}"
alpn = [{alpn}]
"#,
        listen = config.listen,
        uuid = config.uuid,
        password = config.password,
        cert = config.cert_path,
        key = config.key_path,
        cc = config.congestion_control,
        alpn = alpn,
    );

    Ok(toml)
}
