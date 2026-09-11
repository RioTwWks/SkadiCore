//! SOCKS5 (RFC 1928) + user/pass auth (RFC 1929).
//!
//! Парсинг вынесен в `parse` — чистые функции без I/O, пригодные
//! для fuzz-тестирования.

pub mod parse;

use anyhow::{bail, Result};
use parse::{parse_auth, parse_greeting, parse_request, ParseError};
use serde::Deserialize;
use skadi_core::Endpoint;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use subtle::ConstantTimeEq;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, trace, warn};

// ─── Константы ────────────────────────────────────────────────────────
pub const SOCKS5_VERSION: u8 = 0x05;
pub const AUTH_VERSION: u8 = 0x01;
pub const METHOD_NO_AUTH: u8 = 0x00;
pub const METHOD_USER_PASS: u8 = 0x02;
pub const METHOD_NO_ACCEPTABLE: u8 = 0xFF;
pub const CMD_CONNECT: u8 = 0x01;
pub const ATYP_IPV4: u8 = 0x01;
pub const ATYP_DOMAIN: u8 = 0x03;
pub const ATYP_IPV6: u8 = 0x04;
pub const MAX_METHODS: usize = 16;

// Reply codes
pub const REP_SUCCEEDED: u8 = 0x00;
pub const REP_GENERAL_FAILURE: u8 = 0x01;
pub const REP_NOT_ALLOWED: u8 = 0x02;
pub const REP_NETWORK_UNREACHABLE: u8 = 0x03;
pub const REP_HOST_UNREACHABLE: u8 = 0x04;
pub const REP_CONNECTION_REFUSED: u8 = 0x05;
pub const REP_TTL_EXPIRED: u8 = 0x06;
pub const REP_COMMAND_NOT_SUPPORTED: u8 = 0x07;
pub const REP_ADDRESS_NOT_SUPPORTED: u8 = 0x08;

/// Общий таймаут на всю фазу переговоров.
const NEGOTIATION_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_FRAME: usize = 512;

// ─── Конфигурация ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Socks5Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_auth")]
    pub auth: AuthMethod,
    #[serde(default)]
    pub users: Vec<UserCredential>,
}

impl Default for Socks5Config {
    fn default() -> Self {
        Self {
            enabled: false,
            auth: AuthMethod::NoAuth,
            users: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthMethod {
    NoAuth,
    UserPass,
}

fn default_auth() -> AuthMethod {
    AuthMethod::NoAuth
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserCredential {
    pub username: String,
    pub password: String,
}

impl Socks5Config {
    /// Найти пароль пользователя. Линейный поиск — SOCKS5-пользователей
    /// обычно единицы. Если понадобится масштабирование, заменим на HashMap.
    fn lookup(&self, username: &str) -> Option<&str> {
        self.users
            .iter()
            .find(|u| u.username == username)
            .map(|u| u.password.as_str())
    }
}

// ─── Обработчик ───────────────────────────────────────────────────────

pub struct Socks5Handler;

impl Socks5Handler {
    /// Полный цикл переговоров: greeting → метод → аутентификация → запрос.
    /// Возвращает целевой endpoint. Финальный reply НЕ отправляется —
    /// это делает вызывающий код после установки upstream-соединения.
    pub async fn negotiate<S>(client: &mut S, config: &Socks5Config) -> Result<Endpoint>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        match tokio::time::timeout(NEGOTIATION_TIMEOUT, Self::negotiate_inner(client, config)).await
        {
            Ok(result) => result,
            Err(_) => bail!("SOCKS5 negotiation timeout"),
        }
    }

    async fn negotiate_inner<S>(client: &mut S, config: &Socks5Config) -> Result<Endpoint>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let methods = Self::read_greeting(client).await?;
        let selected = Self::select_method(&methods, config);

        trace!(methods = ?methods, selected = selected, "method selection");
        Self::write_all(client, &[SOCKS5_VERSION, selected]).await?;

        if selected == METHOD_NO_ACCEPTABLE {
            bail!("no acceptable auth method");
        }

        if selected == METHOD_USER_PASS {
            Self::perform_auth(client, config).await?;
        }

        Self::read_request(client).await
    }

    /// Отправить финальный reply. Вызывается после попытки подключения
    /// к целевому адресу — success или error.
    pub async fn send_reply<S>(client: &mut S, code: u8, bound: SocketAddr) -> Result<()>
    where
        S: AsyncWrite + Unpin,
    {
        let mut buf = Vec::with_capacity(22);
        buf.push(SOCKS5_VERSION);
        buf.push(code);
        buf.push(0x00); // RSV

        match bound {
            SocketAddr::V4(v4) => {
                buf.push(ATYP_IPV4);
                buf.extend_from_slice(&v4.ip().octets());
                buf.extend_from_slice(&v4.port().to_be_bytes());
            }
            SocketAddr::V6(v6) => {
                buf.push(ATYP_IPV6);
                buf.extend_from_slice(&v6.ip().octets());
                buf.extend_from_slice(&v6.port().to_be_bytes());
            }
        }

        Self::write_all(client, &buf).await
    }

    /// Удобная обёртка для отправки error-reply с dummy BND.
    pub async fn send_error<S>(client: &mut S, code: u8) -> Result<()>
    where
        S: AsyncWrite + Unpin,
    {
        let dummy = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        Self::send_reply(client, code, dummy).await
    }

    // ─── I/O + вызов парсеров ─────────────────────────────────────────

    async fn read_greeting<S>(client: &mut S) -> Result<Vec<u8>>
    where
        S: AsyncRead + Unpin,
    {
        // Читаем сначала 2 байта, чтобы узнать nmethods.
        let mut header = [0u8; 2];
        client.read_exact(&mut header).await?;
        let nmethods = header[1] as usize;

        if nmethods > MAX_METHODS {
            bail!("too many auth methods: {}", nmethods);
        }

        // Дочитываем methods и вызываем парсер на полном буфере.
        let mut buf = Vec::with_capacity(2 + nmethods);
        buf.extend_from_slice(&header);
        buf.resize(2 + nmethods, 0);
        client.read_exact(&mut buf[2..]).await?;

        let (methods, _) =
            parse_greeting(&buf).map_err(|e| anyhow::anyhow!("greeting parse: {}", e))?;
        Ok(methods)
    }

    fn select_method(methods: &[u8], config: &Socks5Config) -> u8 {
        let wanted = match config.auth {
            AuthMethod::NoAuth => METHOD_NO_AUTH,
            AuthMethod::UserPass => METHOD_USER_PASS,
        };
        if methods.contains(&wanted) {
            wanted
        } else {
            METHOD_NO_ACCEPTABLE
        }
    }

    async fn perform_auth<S>(client: &mut S, config: &Socks5Config) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        // Читаем header, потом uname, потом plen, потом passwd.
        let mut header = [0u8; 2];
        client.read_exact(&mut header).await?;
        let ulen = header[1] as usize;

        if ulen == 0 {
            bail!("empty username");
        }

        let mut buf = Vec::with_capacity(2 + ulen + 1 + 255);
        buf.extend_from_slice(&header);
        buf.resize(2 + ulen, 0);
        client.read_exact(&mut buf[2..]).await?;

        let mut plen_buf = [0u8; 1];
        client.read_exact(&mut plen_buf).await?;
        let plen = plen_buf[0] as usize;

        if plen == 0 {
            bail!("empty password");
        }

        buf.push(plen_buf[0]);
        let plen_offset = buf.len();
        buf.resize(plen_offset + plen, 0);
        client.read_exact(&mut buf[plen_offset..]).await?;

        let ((uname, passwd), _) =
            parse_auth(&buf).map_err(|e| anyhow::anyhow!("auth parse: {}", e))?;

        // Даже если пользователя нет — сравниваем с заглушкой,
        // чтобы время ответа не выдавало существование логина.
        let expected = config.lookup(&uname).unwrap_or("__no_such_user__");
        let ok = constant_time_eq(expected, &passwd);

        let status = if ok { 0x00 } else { 0x01 };
        Self::write_all(client, &[AUTH_VERSION, status]).await?;

        if !ok {
            warn!(username = %uname, "SOCKS5 auth failed");
            bail!("authentication failed");
        }

        debug!(username = %uname, "SOCKS5 auth ok");
        Ok(())
    }

    async fn read_request<S>(client: &mut S) -> Result<Endpoint>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        // Читаем 4 байта header, потом — в зависимости от atyp.
        let mut header = [0u8; 4];
        client.read_exact(&mut header).await?;
        let atyp = header[3];

        let mut buf = Vec::with_capacity(MAX_FRAME);
        buf.extend_from_slice(&header);

        match atyp {
            ATYP_IPV4 => {
                buf.resize(10, 0);
                client.read_exact(&mut buf[4..]).await?;
            }
            ATYP_IPV6 => {
                buf.resize(22, 0);
                client.read_exact(&mut buf[4..]).await?;
            }
            ATYP_DOMAIN => {
                let mut len_buf = [0u8; 1];
                client.read_exact(&mut len_buf).await?;
                let dlen = len_buf[0] as usize;

                if dlen == 0 {
                    let _ = Self::send_error(client, REP_ADDRESS_NOT_SUPPORTED).await;
                    bail!("empty domain");
                }

                buf.push(len_buf[0]);
                let after = buf.len();
                buf.resize(after + dlen + 2, 0);
                client.read_exact(&mut buf[after..]).await?;
            }
            other => {
                let _ = Self::send_error(client, REP_ADDRESS_NOT_SUPPORTED).await;
                bail!("unsupported address type: 0x{:02x}", other);
            }
        }

        let (endpoint, _) = parse_request(&buf).map_err(|e| {
            // Для команд и версий отправляем осмысленный reply.
            let code = match e {
                ParseError::UnsupportedCommand(_) => REP_COMMAND_NOT_SUPPORTED,
                ParseError::UnsupportedAddressType(_) => REP_ADDRESS_NOT_SUPPORTED,
                _ => REP_GENERAL_FAILURE,
            };
            anyhow::anyhow!("request parse (reply 0x{:02x}): {}", code, e)
        })?;

        debug!(target = %endpoint, "SOCKS5 CONNECT request");
        Ok(endpoint)
    }

    async fn write_all<S>(client: &mut S, buf: &[u8]) -> Result<()>
    where
        S: AsyncWrite + Unpin,
    {
        client.write_all(buf).await?;
        client.flush().await?;
        Ok(())
    }
}

/// Сравнение за постоянное время. Разная длина утекает — но длина
/// пароля в SOCKS5 не считается секретом.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}
