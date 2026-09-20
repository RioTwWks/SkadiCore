//! Рендер конфигов для `tuic-server` / `tuic-client`.

use super::config::{TuicClientConfig, TuicServerConfig};
use anyhow::Result;

/// Собрать минимальный `config.toml` для tuic-server.
pub fn render_server_toml(config: &TuicServerConfig) -> Result<String> {
    let alpn = config
        .alpn
        .iter()
        .map(|s| format!("\"{s}\""))
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

/// Собрать JSON-конфиг для `tuic-client` (SOCKS5 inbound).
pub fn render_client_json(config: &TuicClientConfig) -> Result<String> {
    let alpn: Vec<String> = config.alpn.iter().map(|s| format!("\"{s}\"")).collect();
    let alpn = alpn.join(", ");

    let mut relay = format!(
        r#"    "server": "{server}",
    "uuid": "{uuid}",
    "password": "{password}",
    "udp_relay_mode": "{udp}",
    "congestion_control": "{cc}",
    "alpn": [{alpn}]"#,
        server = json_escape(&config.server),
        uuid = json_escape(&config.uuid),
        password = json_escape(&config.password),
        udp = json_escape(&config.udp_relay_mode),
        cc = json_escape(&config.congestion_control),
        alpn = alpn,
    );

    if let Some(ip) = &config.ip {
        if !ip.trim().is_empty() {
            relay.push_str(&format!(",\n    \"ip\": \"{}\"", json_escape(ip.trim())));
        }
    }
    if config.allow_insecure {
        // Совместимость с tuic-client / форками (Itsusinn и др.).
        relay.push_str(",\n    \"allow_insecure\": true");
    }

    let json = format!(
        r#"{{
  "relay": {{
{relay}
  }},
  "local": {{
    "server": "{local}"
  }},
  "log_level": "warn"
}}
"#,
        local = json_escape(&config.socks5_listen),
    );

    Ok(json)
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_json_contains_socks() {
        let cfg = TuicClientConfig {
            server: "example.com:8443".into(),
            uuid: "00000000-0000-0000-0000-000000000001".into(),
            password: "secret".into(),
            socks5_listen: "127.0.0.1:1080".into(),
            ip: Some("1.2.3.4".into()),
            congestion_control: "bbr".into(),
            alpn: vec!["h3".into()],
            udp_relay_mode: "native".into(),
            allow_insecure: true,
        };
        let json = render_client_json(&cfg).unwrap();
        assert!(json.contains("\"local\""));
        assert!(json.contains("127.0.0.1:1080"));
        assert!(json.contains("allow_insecure"));
        assert!(json.contains("1.2.3.4"));
    }
}
