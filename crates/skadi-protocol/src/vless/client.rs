//! VLESS client handshake (outbound к удалённому прокси).

use anyhow::{bail, Context, Result};
use skadi_core::Endpoint;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::parse::{
    build_response_header, build_tcp_domain_request, build_tcp_request, encode_port_address,
    CMD_TCP, VLESS_VERSION,
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub struct VlessClient;

impl VlessClient {
    /// Отправить VLESS TCP-запрос и прочитать 2-байтовый ответ сервера.
    pub async fn handshake_tcp<S>(stream: &mut S, uuid: &[u8; 16], target: &Endpoint) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        match tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            Self::handshake_tcp_inner(stream, uuid, target),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => bail!("VLESS client handshake timeout"),
        }
    }

    async fn handshake_tcp_inner<S>(
        stream: &mut S,
        uuid: &[u8; 16],
        target: &Endpoint,
    ) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let request = build_tcp_endpoint_request(uuid, target);
        stream
            .write_all(&request)
            .await
            .context("failed to write VLESS request")?;

        let mut response = [0u8; 2];
        stream
            .read_exact(&mut response)
            .await
            .context("failed to read VLESS response header")?;

        let expected = build_response_header(VLESS_VERSION);
        if response != expected {
            bail!(
                "unexpected VLESS response: {:02x} {:02x}",
                response[0],
                response[1]
            );
        }

        Ok(())
    }
}

/// Собрать VLESS TCP-запрос к произвольному `Endpoint`.
pub fn build_tcp_endpoint_request(uuid: &[u8; 16], target: &Endpoint) -> Vec<u8> {
    match target {
        Endpoint::Ip(addr) => match addr.ip() {
            std::net::IpAddr::V4(ip) => build_tcp_request(uuid, ip, addr.port()),
            std::net::IpAddr::V6(_) => {
                let mut buf = Vec::with_capacity(40);
                buf.push(VLESS_VERSION);
                buf.extend_from_slice(uuid);
                buf.push(0);
                buf.push(CMD_TCP);
                buf.extend_from_slice(&encode_port_address(target));
                buf
            }
        },
        Endpoint::Domain(domain, port) => build_tcp_domain_request(uuid, domain, *port),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vless::parse::parse_request;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn build_endpoint_request_ipv4_roundtrip() {
        let uuid = [0xAB; 16];
        let target = Endpoint::Ip(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 443));
        let buf = build_tcp_endpoint_request(&uuid, &target);
        let (req, len) = parse_request(&buf).unwrap();
        assert_eq!(len, buf.len());
        assert_eq!(req.command, CMD_TCP);
        assert_eq!(req.target, target);
    }

    #[test]
    fn build_endpoint_request_domain_roundtrip() {
        let uuid = [0xCD; 16];
        let target = Endpoint::Domain("example.com".into(), 80);
        let buf = build_tcp_endpoint_request(&uuid, &target);
        let (req, len) = parse_request(&buf).unwrap();
        assert_eq!(len, buf.len());
        assert_eq!(req.target, target);
    }
}
