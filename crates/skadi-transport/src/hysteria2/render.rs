//! Рендер YAML для `hysteria server` / `hysteria client` и share URI.

use super::config::{Hysteria2ClientConfig, Hysteria2ServerConfig};
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

/// Собрать `config.yaml` для Hysteria 2 client (SOCKS5 ± HTTP).
pub fn render_client_yaml(config: &Hysteria2ClientConfig) -> Result<String> {
    let mut yaml = format!(
        "server: {}\nauth: {}\n\nsocks5:\n  listen: {}\n",
        yaml_quote(&config.server),
        yaml_quote(&config.password),
        yaml_quote(&config.socks5_listen),
    );

    if let Some(http) = &config.http_listen {
        if !http.trim().is_empty() {
            yaml.push_str(&format!("\nhttp:\n  listen: {}\n", yaml_quote(http.trim())));
        }
    }

    let need_tls = config.insecure
        || config.ca_file.is_some()
        || config.sni.is_some()
        || config.pin_sha256.is_some();
    if need_tls {
        yaml.push_str("\ntls:\n");
        if let Some(sni) = &config.sni {
            if !sni.trim().is_empty() {
                yaml.push_str(&format!("  sni: {}\n", yaml_quote(sni.trim())));
            }
        }
        if let Some(ca) = &config.ca_file {
            if !ca.trim().is_empty() {
                yaml.push_str(&format!("  ca: {}\n", yaml_quote(ca.trim())));
            }
        }
        if config.insecure {
            yaml.push_str("  insecure: true\n");
        }
        if let Some(pin) = &config.pin_sha256 {
            if !pin.trim().is_empty() {
                yaml.push_str(&format!("  pinSHA256: {}\n", yaml_quote(pin.trim())));
            }
        }
    }

    if config.bandwidth_up.is_some() || config.bandwidth_down.is_some() {
        yaml.push_str("\nbandwidth:\n");
        if let Some(up) = &config.bandwidth_up {
            yaml.push_str(&format!("  up: {}\n", yaml_quote(up)));
        }
        if let Some(down) = &config.bandwidth_down {
            yaml.push_str(&format!("  down: {}\n", yaml_quote(down)));
        }
    }

    Ok(yaml)
}

/// Share URI `hy2://…` (совместим с официальным клиентом).
pub fn render_share_uri(config: &Hysteria2ClientConfig) -> String {
    let password = urlencoding_simple(&config.password);
    let mut uri = format!("hy2://{password}@{}", config.server);
    let mut q = Vec::new();
    if let Some(sni) = &config.sni {
        if !sni.is_empty() {
            q.push(format!("sni={}", urlencoding_simple(sni)));
        }
    }
    if config.insecure {
        q.push("insecure=1".to_string());
    }
    if !q.is_empty() {
        uri.push('?');
        uri.push_str(&q.join("&"));
    }
    uri
}

fn yaml_quote(value: &str) -> String {
    // Кавычки нужны для IPv6 `:port` и спецсимволов пароля.
    if value.is_empty()
        || value.contains(':')
        || value.contains('#')
        || value.contains(' ')
        || value.contains('"')
        || value.contains('\'')
        || value.starts_with(['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'])
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_yaml_socks5() {
        let cfg = Hysteria2ClientConfig {
            server: "example.com:443".into(),
            password: "secret".into(),
            socks5_listen: "127.0.0.1:1080".into(),
            http_listen: None,
            sni: Some("example.com".into()),
            ca_file: None,
            insecure: true,
            pin_sha256: None,
            bandwidth_up: None,
            bandwidth_down: None,
        };
        let yaml = render_client_yaml(&cfg).unwrap();
        assert!(yaml.contains("server:"));
        assert!(yaml.contains("socks5:"));
        assert!(yaml.contains("insecure: true"));
        let uri = render_share_uri(&cfg);
        assert!(uri.starts_with("hy2://"));
        assert!(uri.contains("insecure=1"));
    }
}
