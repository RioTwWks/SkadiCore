//! DNS-over-TLS через VLESS TCP + TLS к upstream DoT-серверу (RFC 7858).

use crate::doh::system_tls_connector;
use crate::outbound::Outbound;
use anyhow::{bail, Context, Result};
use rustls_pki_types::ServerName;
use skadi_core::Endpoint;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio_rustls::TlsConnector;
use tracing::debug;

#[derive(Clone)]
pub struct DotConfig {
    pub host: String,
    pub port: u16,
    pub tls_server_name: String,
    connector: TlsConnector,
}

impl DotConfig {
    pub fn from_server_url(value: &str) -> Result<Self> {
        let parsed = parse_dot_upstream(value)?;
        let connector = system_tls_connector()?;
        Ok(Self {
            host: parsed.host,
            port: parsed.port,
            tls_server_name: parsed.tls_server_name,
            connector,
        })
    }

    pub async fn query(&self, outbound: &Outbound, query: &[u8]) -> Result<Vec<u8>> {
        if query.is_empty() {
            bail!("empty DNS query");
        }
        if query.len() > 65_535 {
            bail!("DNS query too large for DoT");
        }

        let target = Endpoint::Domain(self.host.clone(), self.port);
        let tcp = outbound
            .open_tcp(&target)
            .await
            .context("VLESS connect to DoT upstream failed")?;

        let server_name = ServerName::try_from(self.tls_server_name.clone())
            .map_err(|_| anyhow::anyhow!("invalid DoT TLS server name"))?;
        let mut tls = self
            .connector
            .connect(server_name, tcp)
            .await
            .context("DoT TLS handshake failed")?;

        let framed = encode_dot_frame(query);
        tls.write_all(&framed)
            .await
            .context("DoT query write failed")?;

        let response = read_dot_frame(&mut tls)
            .await
            .context("DoT response read failed")?;
        debug!(
            query_len = query.len(),
            response_len = response.len(),
            "DoT query ok"
        );
        Ok(response)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedDotUpstream {
    host: String,
    port: u16,
    tls_server_name: String,
}

fn parse_dot_upstream(value: &str) -> Result<ParsedDotUpstream> {
    let value = value
        .strip_prefix("tls://")
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string());
    if value.is_empty() {
        bail!("empty DoT upstream");
    }

    let (host, port) = if let Some((host, port_str)) = value.rsplit_once(':') {
        if host.is_empty() {
            bail!("invalid DoT upstream host in {}", value);
        }
        let port = port_str
            .parse()
            .with_context(|| format!("invalid DoT upstream port in {}", value))?;
        (host.to_string(), port)
    } else {
        (value.clone(), 853)
    };

    Ok(ParsedDotUpstream {
        host: host.clone(),
        port,
        tls_server_name: host,
    })
}

fn encode_dot_frame(query: &[u8]) -> Vec<u8> {
    let len = query.len();
    let mut frame = vec![(len >> 8) as u8, (len & 0xff) as u8];
    frame.extend_from_slice(query);
    frame
}

async fn read_dot_frame<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 2];
    stream
        .read_exact(&mut len_buf)
        .await
        .context("DoT response length EOF")?;
    let len = ((len_buf[0] as usize) << 8) | len_buf[1] as usize;
    if len == 0 {
        bail!("empty DoT response");
    }
    if len > 65_536 {
        bail!("DoT response too large");
    }

    let mut body = vec![0u8; len];
    stream
        .read_exact(&mut body)
        .await
        .context("DoT response body EOF")?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_host_only_defaults_to_853() {
        let parsed = parse_dot_upstream("one.one.one.one").unwrap();
        assert_eq!(parsed.host, "one.one.one.one");
        assert_eq!(parsed.port, 853);
        assert_eq!(parsed.tls_server_name, "one.one.one.one");
    }

    #[test]
    fn parse_tls_url_with_port() {
        let parsed = parse_dot_upstream("tls://1.1.1.1:853").unwrap();
        assert_eq!(parsed.host, "1.1.1.1");
        assert_eq!(parsed.port, 853);
    }

    #[test]
    fn encode_frame_prefixes_length() {
        let query = [0xab, 0xcd];
        let frame = encode_dot_frame(&query);
        assert_eq!(frame, [0x00, 0x02, 0xab, 0xcd]);
    }
}
