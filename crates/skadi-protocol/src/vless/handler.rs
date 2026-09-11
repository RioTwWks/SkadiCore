use anyhow::{bail, Context, Result};
use skadi_core::Endpoint;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, warn};

use super::addons::{parse_addons, validate_flow};
use super::config::VlessConfig;
use super::parse::{
    build_response_header, parse_request, ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6, MAX_ADDONS,
    VLESS_VERSION,
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Результат VLESS handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VlessHandshake {
    pub command: u8,
    pub target: Endpoint,
}

pub struct VlessHandler;

impl VlessHandler {
    /// Выполнить VLESS handshake: прочитать заголовок, аутентифицировать
    /// пользователя и вернуть команду с целевым endpoint.
    ///
    /// При неверном UUID или неподдерживаемом flow соединение молча
    /// закрывается — как того требует спецификация.
    pub async fn handshake<S>(client: &mut S, config: &VlessConfig) -> Result<VlessHandshake>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        match tokio::time::timeout(HANDSHAKE_TIMEOUT, Self::handshake_inner(client, config)).await {
            Ok(result) => result,
            Err(_) => bail!("VLESS handshake timeout"),
        }
    }

    async fn handshake_inner<S>(client: &mut S, config: &VlessConfig) -> Result<VlessHandshake>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        // Читаем фиксированную часть: version + uuid + addons_len = 18.
        let mut head = [0u8; 18];
        client
            .read_exact(&mut head)
            .await
            .context("failed to read VLESS header prefix")?;

        let version = head[0];
        if version != VLESS_VERSION {
            bail!("unsupported VLESS version: 0x{:02x}", version);
        }

        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&head[1..17]);

        let addons_len = head[17] as usize;
        if addons_len > MAX_ADDONS {
            bail!("VLESS addons too large: {}", addons_len);
        }

        // Дочитываем addons + command + port + atyp.
        let tail_len = addons_len + 1 + 2 + 1;
        let mut tail = vec![0u8; tail_len];
        client
            .read_exact(&mut tail)
            .await
            .context("failed to read VLESS header tail")?;

        let mut full = Vec::with_capacity(18 + tail_len);
        full.extend_from_slice(&head);
        full.extend_from_slice(&tail);

        // Аутентификация до чтения адреса — экономим работу на мусорных
        // соединениях и не даём зонду увидеть разницу в таймингах.
        let user = config.authenticate(&uuid);
        if user.is_none() {
            warn!(uuid = ?uuid, "VLESS auth failed, closing silently");
            bail!("authentication failed");
        }
        let user = user.unwrap();

        // Дочитываем адрес в зависимости от ATYP.
        let atyp = full[full.len() - 1];
        let addr_len = match atyp {
            ATYP_IPV4 => 4,
            ATYP_IPV6 => 16,
            ATYP_DOMAIN => {
                let mut dlen = [0u8; 1];
                client.read_exact(&mut dlen).await?;
                full.push(dlen[0]);
                dlen[0] as usize
            }
            other => bail!("unsupported address type: 0x{:02x}", other),
        };

        if addr_len > super::parse::MAX_DOMAIN {
            bail!("address too long: {}", addr_len);
        }

        let addr_start = full.len();
        full.resize(addr_start + addr_len, 0);
        client
            .read_exact(&mut full[addr_start..])
            .await
            .context("failed to read VLESS address")?;

        let (request, _) =
            parse_request(&full).map_err(|e| anyhow::anyhow!("VLESS parse: {}", e))?;

        let addons =
            parse_addons(&request.addons).map_err(|e| anyhow::anyhow!("VLESS addons: {}", e))?;

        if let Err(flow_err) = validate_flow(addons.flow.as_deref(), user.flow.as_deref()) {
            warn!(flow_error = ?flow_err, "unsupported VLESS flow, closing silently");
            bail!("unsupported flow");
        }

        debug!(
            command = request.command,
            addons_len = request.addons.len(),
            flow = ?addons.flow,
            target = %request.target,
            "VLESS request decoded"
        );

        // Отправляем ответный заголовок — 2 байта.
        let resp = build_response_header(VLESS_VERSION);
        client.write_all(&resp).await?;
        client.flush().await?;

        Ok(VlessHandshake {
            command: request.command,
            target: request.target,
        })
    }
}
