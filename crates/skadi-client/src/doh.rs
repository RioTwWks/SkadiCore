//! DNS-over-HTTPS через VLESS TCP + TLS к upstream DoH-серверу.

use crate::outbound::Outbound;
use anyhow::{bail, Context, Result};
use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls_pki_types::ServerName;
use skadi_core::Endpoint;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio_rustls::TlsConnector;
use tracing::debug;

#[derive(Clone)]
pub struct DohConfig {
    pub host: String,
    pub port: u16,
    pub path: String,
    pub tls_server_name: String,
    connector: TlsConnector,
}

impl DohConfig {
    pub fn from_server_url(url: &str) -> Result<Self> {
        let parsed = parse_doh_url(url)?;
        let connector = system_tls_connector()?;
        Ok(Self {
            host: parsed.host,
            port: parsed.port,
            path: parsed.path,
            tls_server_name: parsed.tls_server_name,
            connector,
        })
    }

    pub async fn query(&self, outbound: &Outbound, query: &[u8]) -> Result<Vec<u8>> {
        if query.is_empty() {
            bail!("empty DNS query");
        }
        if query.len() > 4096 {
            bail!("DNS query too large for DoH");
        }

        let target = Endpoint::Domain(self.host.clone(), self.port);
        let tcp = outbound
            .open_tcp(&target)
            .await
            .context("VLESS connect to DoH upstream failed")?;

        let server_name = ServerName::try_from(self.tls_server_name.clone())
            .map_err(|_| anyhow::anyhow!("invalid DoH TLS server name"))?;
        let mut tls = self
            .connector
            .connect(server_name, tcp)
            .await
            .context("DoH TLS handshake failed")?;

        let request = build_doh_post(&self.tls_server_name, &self.path, query);
        tls.write_all(&request)
            .await
            .context("DoH HTTP request write failed")?;

        let response = read_doh_response(&mut tls)
            .await
            .context("DoH HTTP response read failed")?;
        debug!(
            query_len = query.len(),
            response_len = response.len(),
            "DoH query ok"
        );
        Ok(response)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedDohUrl {
    host: String,
    port: u16,
    path: String,
    tls_server_name: String,
}

fn parse_doh_url(url: &str) -> Result<ParsedDohUrl> {
    let rest = url
        .strip_prefix("https://")
        .with_context(|| format!("DoH server must be an https URL, got {}", url))?;
    if rest.is_empty() {
        bail!("empty DoH URL");
    }

    let (authority, path) = match rest.split_once('/') {
        Some((auth, path)) if !path.is_empty() => (auth, format!("/{}", path)),
        _ => (rest, "/dns-query".to_string()),
    };

    let (host, port) = if let Some((host, port_str)) = authority.rsplit_once(':') {
        if host.is_empty() {
            bail!("invalid DoH URL host in {}", url);
        }
        let port = port_str
            .parse()
            .with_context(|| format!("invalid DoH URL port in {}", url))?;
        (host.to_string(), port)
    } else {
        (authority.to_string(), 443)
    };

    Ok(ParsedDohUrl {
        host: host.clone(),
        port,
        path,
        tls_server_name: host,
    })
}

fn build_doh_post(host: &str, path: &str, query: &[u8]) -> Vec<u8> {
    let mut request = format!(
        "POST {path} HTTP/1.1\r\n\
Host: {host}\r\n\
Content-Type: application/dns-message\r\n\
Accept: application/dns-message\r\n\
Content-Length: {len}\r\n\
Connection: close\r\n\
\r\n",
        path = path,
        host = host,
        len = query.len()
    )
    .into_bytes();
    request.extend_from_slice(query);
    request
}

async fn read_doh_response<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];

    while buf.windows(4).all(|w| w != b"\r\n\r\n") {
        let n = stream.read(&mut chunk).await.context("DoH response EOF")?;
        if n == 0 {
            bail!("incomplete DoH HTTP headers");
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 64 * 1024 {
            bail!("DoH HTTP headers too large");
        }
    }

    let header_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("DoH HTTP headers malformed")?
        + 4;
    let headers = &buf[..header_end];
    let status_line = headers
        .split(|&b| b == b'\n')
        .next()
        .context("empty DoH HTTP response")?;
    if !status_line.starts_with(b"HTTP/1.1 200") && !status_line.starts_with(b"HTTP/1.0 200") {
        bail!(
            "DoH upstream returned non-200 status: {}",
            String::from_utf8_lossy(status_line)
        );
    }

    let content_length = parse_content_length(headers)?;
    let body_start = header_end;
    let needed = body_start + content_length;

    while buf.len() < needed {
        let n = stream
            .read(&mut chunk)
            .await
            .context("DoH response body EOF")?;
        if n == 0 {
            bail!("truncated DoH response body");
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > body_start + 65_536 {
            bail!("DoH response body too large");
        }
    }

    Ok(buf[body_start..needed].to_vec())
}

fn parse_content_length(headers: &[u8]) -> Result<usize> {
    for line in headers.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(value) = line
            .strip_prefix(b"Content-Length:")
            .or_else(|| line.strip_prefix(b"content-length:"))
        {
            let value = value
                .trim_ascii()
                .iter()
                .copied()
                .filter(|b| *b != b' ')
                .collect::<Vec<_>>();
            let len = String::from_utf8(value)
                .context("invalid Content-Length encoding")?
                .parse()
                .context("invalid Content-Length value")?;
            return Ok(len);
        }
    }
    bail!("DoH response missing Content-Length");
}

pub(crate) fn system_tls_connector() -> Result<TlsConnector> {
    let mut roots = RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    for err in native.errors {
        debug!(error = %err, "failed to load native TLS certificate");
    }
    for cert in native.certs {
        roots.add(cert).context("invalid native TLS certificate")?;
    }

    let provider = rustls::crypto::ring::default_provider();
    let config = ClientConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .context("unsupported TLS protocol versions")?
        .with_root_certificates(roots)
        .with_no_client_auth();

    Ok(TlsConnector::from(Arc::new(config)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cloudflare_doh_url() {
        let parsed = parse_doh_url("https://cloudflare-dns.com/dns-query").unwrap();
        assert_eq!(parsed.host, "cloudflare-dns.com");
        assert_eq!(parsed.port, 443);
        assert_eq!(parsed.path, "/dns-query");
    }

    #[test]
    fn parse_ip_doh_url_with_port() {
        let parsed = parse_doh_url("https://1.1.1.1:443/dns-query").unwrap();
        assert_eq!(parsed.host, "1.1.1.1");
        assert_eq!(parsed.port, 443);
        assert_eq!(parsed.tls_server_name, "1.1.1.1");
    }

    #[test]
    fn rejects_non_https_url() {
        assert!(parse_doh_url("http://1.1.1.1/dns-query").is_err());
    }

    #[test]
    fn build_post_includes_dns_payload() {
        let query = [0x12, 0x34];
        let request = build_doh_post("cloudflare-dns.com", "/dns-query", &query);
        let text = String::from_utf8_lossy(&request[..request.len() - 2]);
        assert!(text.contains("POST /dns-query HTTP/1.1"));
        assert!(text.contains("Content-Length: 2"));
        assert_eq!(&request[request.len() - 2..], query);
    }

    #[test]
    fn parse_content_length_from_headers() {
        let headers = b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\n\r\n";
        assert_eq!(parse_content_length(headers).unwrap(), 12);
    }
}
