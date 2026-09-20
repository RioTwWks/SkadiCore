//! Рендер конфигов для `tuic-server` / `tuic-client` (Itsusinn TUIC 1.7+).

use super::config::{TuicClientConfig, TuicServerConfig};
use anyhow::Result;

/// Собрать `config.toml` для Itsusinn `tuic-server`.
pub fn render_server_toml(config: &TuicServerConfig) -> Result<String> {
    let alpn = config
        .alpn
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(", ");

    // dual_stack=false: на CI/контейнерах без IPv6 иначе EOPNOTSUPP.
    // drop_loopback/private=false: Itsusinn 1.7+ по умолчанию режет LAN/loopback
    // (e2e и типичный VPN-сценарий через allow_private на стороне Skadi).
    let toml = format!(
        r#"log_level = "warn"
server = "{listen}"
udp_relay_ipv6 = false
dual_stack = false

[users]
"{uuid}" = "{password}"

[tls]
certificate = "{cert}"
private_key = "{key}"
alpn = [{alpn}]

[quic.congestion_control]
controller = "{cc}"

[experimental]
drop_loopback = false
drop_private = false
"#,
        listen = config.listen,
        uuid = config.uuid,
        password = escape_toml_string(&config.password),
        cert = config.cert_path,
        key = config.key_path,
        alpn = alpn,
        cc = config.congestion_control,
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
        // Itsusinn tuic-client: `skip_cert_verify`.
        relay.push_str(",\n    \"skip_cert_verify\": true");
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

fn escape_toml_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_toml_itsusinn_shape() {
        let cfg = TuicServerConfig {
            listen: "127.0.0.1:8443".parse().unwrap(),
            uuid: "00000000-0000-0000-0000-000000000001".into(),
            password: "secret".into(),
            cert_path: "/tmp/cert.pem".into(),
            key_path: "/tmp/key.pem".into(),
            congestion_control: "bbr".into(),
            alpn: vec!["h3".into()],
        };
        let toml = render_server_toml(&cfg).unwrap();
        assert!(toml.contains("server = \"127.0.0.1:8443\""));
        assert!(toml.contains("[users]"));
        assert!(toml.contains("dual_stack = false"));
        assert!(toml.contains("drop_loopback = false"));
        assert!(toml.contains("drop_private = false"));
        assert!(!toml.contains("[server]"));
    }

    #[test]
    fn client_json_skip_cert_verify() {
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
        assert!(json.contains("skip_cert_verify"));
        assert!(json.contains("1.2.3.4"));
    }
}
