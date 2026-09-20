//! Унифицированный inbound handshake для SOCKS5 / VLESS.
//!
//! Новый протокол = `impl InboundHandler`, без правки ядра `handle_connection`
//! на этапе переговоров (специальные команды — BIND/MUX/UDP — по-прежнему
//! разбираются по `InboundRequest.command`).

use anyhow::Result;
use skadi_core::{Endpoint, Protocol};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::socks5::{Socks5Config, Socks5Handler, Socks5Request, CMD_CONNECT};
use crate::vless::{VlessConfig, VlessHandler, VlessHandshake, CMD_TCP};

/// Результат успешного inbound handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundRequest {
    pub protocol: Protocol,
    /// Команда протокола (`CMD_CONNECT` / `CMD_TCP` / `CMD_UDP` / …).
    pub command: u8,
    pub target: Endpoint,
}

impl InboundRequest {
    pub fn from_socks5(req: Socks5Request) -> Self {
        Self {
            protocol: Protocol::Socks5,
            command: req.command,
            target: req.target,
        }
    }

    pub fn from_vless(hs: VlessHandshake) -> Self {
        Self {
            protocol: Protocol::Vless,
            command: hs.command,
            target: hs.target,
        }
    }

    /// TCP CONNECT (SOCKS5) / TCP (VLESS).
    pub fn is_tcp_connect(&self) -> bool {
        match self.protocol {
            Protocol::Socks5 => self.command == CMD_CONNECT,
            Protocol::Vless => self.command == CMD_TCP,
        }
    }
}

/// Трейт inbound-протокола: stream → [`InboundRequest`].
///
/// Конфиг ассоциирован с типом handler'а (`Socks5Config` / `VlessConfig`).
pub trait InboundHandler {
    type Config: ?Sized;

    fn handshake<S>(
        stream: &mut S,
        config: &Self::Config,
    ) -> impl std::future::Future<Output = Result<InboundRequest>> + Send
    where
        S: AsyncRead + AsyncWrite + Unpin + Send;
}

impl InboundHandler for Socks5Handler {
    type Config = Socks5Config;

    async fn handshake<S>(stream: &mut S, config: &Self::Config) -> Result<InboundRequest>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
    {
        let req = Socks5Handler::negotiate(stream, config).await?;
        Ok(InboundRequest::from_socks5(req))
    }
}

impl InboundHandler for VlessHandler {
    type Config = VlessConfig;

    async fn handshake<S>(stream: &mut S, config: &Self::Config) -> Result<InboundRequest>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
    {
        // Inherent `VlessHandler::handshake` → `VlessHandshake` (не этот трейт).
        let hs = VlessHandler::handshake(stream, config).await?;
        Ok(InboundRequest::from_vless(hs))
    }
}

/// Диспетчер по [`Protocol`]: вызывает нужный [`InboundHandler`].
pub async fn handshake_inbound<S>(
    protocol: Protocol,
    stream: &mut S,
    socks5: &Socks5Config,
    vless: &VlessConfig,
) -> Result<InboundRequest>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    match protocol {
        Protocol::Socks5 => <Socks5Handler as InboundHandler>::handshake(stream, socks5).await,
        Protocol::Vless => <VlessHandler as InboundHandler>::handshake(stream, vless).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skadi_core::Endpoint;
    use std::net::{Ipv4Addr, SocketAddr};

    use crate::socks5::CMD_BIND;
    use crate::vless::{CMD_MUX, CMD_UDP};

    #[test]
    fn inbound_request_tcp_connect_flags() {
        let addr: SocketAddr = (Ipv4Addr::LOCALHOST, 80).into();
        let socks = InboundRequest::from_socks5(Socks5Request {
            command: CMD_CONNECT,
            target: Endpoint::Ip(addr),
        });
        assert!(socks.is_tcp_connect());

        let socks_bind = InboundRequest::from_socks5(Socks5Request {
            command: CMD_BIND,
            target: Endpoint::Ip(addr),
        });
        assert!(!socks_bind.is_tcp_connect());

        let vless = InboundRequest::from_vless(VlessHandshake {
            command: CMD_TCP,
            target: Endpoint::Ip(addr),
        });
        assert!(vless.is_tcp_connect());

        let vless_udp = InboundRequest::from_vless(VlessHandshake {
            command: CMD_UDP,
            target: Endpoint::Ip(addr),
        });
        assert!(!vless_udp.is_tcp_connect());

        let vless_mux = InboundRequest::from_vless(VlessHandshake {
            command: CMD_MUX,
            target: Endpoint::Ip(addr),
        });
        assert!(!vless_mux.is_tcp_connect());
    }
}
