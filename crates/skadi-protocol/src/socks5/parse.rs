//! Чистые функции парсинга SOCKS5.
//!
//! Никакого I/O — только работа с байтами. Это делает их пригодными
//! для fuzz-тестирования и юнит-тестов без сети.
//!
//! Каждая функция возвращает `(результат, потреблённые_байты)`.
//! Вызывающий код сам решает, достаточно ли данных, и читает ещё.

use crate::socks5::{
    Socks5Request, ATYP_DOMAIN, ATYP_IPV4, ATYP_IPV6, AUTH_VERSION, CMD_BIND, CMD_CONNECT,
    CMD_UDP_ASSOCIATE, MAX_METHODS, SOCKS5_VERSION,
};
use skadi_core::Endpoint;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("not enough data: need {need}, have {have}")]
    Incomplete { need: usize, have: usize },

    #[error("invalid SOCKS version: 0x{0:02x}")]
    BadVersion(u8),

    #[error("zero auth methods offered")]
    ZeroMethods,

    #[error("too many auth methods: {0}")]
    TooManyMethods(usize),

    #[error("unsupported auth version: 0x{0:02x}")]
    BadAuthVersion(u8),

    #[error("empty username")]
    EmptyUsername,

    #[error("empty password")]
    EmptyPassword,

    #[error("non-zero RSV byte: 0x{0:02x}")]
    NonZeroRsv(u8),

    #[error("unsupported command: 0x{0:02x}")]
    UnsupportedCommand(u8),

    #[error("unsupported address type: 0x{0:02x}")]
    UnsupportedAddressType(u8),

    #[error("empty domain name")]
    EmptyDomain,

    #[error("invalid UTF-8 in {0}")]
    InvalidUtf8(&'static str),
}

// ─── Greeting ─────────────────────────────────────────────────────────

/// Разбор приветствия клиента: `[ver, nmethods, methods...]`.
pub fn parse_greeting(input: &[u8]) -> Result<(Vec<u8>, usize), ParseError> {
    if input.len() < 2 {
        return Err(ParseError::Incomplete {
            need: 2,
            have: input.len(),
        });
    }

    let version = input[0];
    let nmethods = input[1] as usize;

    if version != SOCKS5_VERSION {
        return Err(ParseError::BadVersion(version));
    }
    if nmethods == 0 {
        return Err(ParseError::ZeroMethods);
    }
    if nmethods > MAX_METHODS {
        return Err(ParseError::TooManyMethods(nmethods));
    }

    let total = 2 + nmethods;
    if input.len() < total {
        return Err(ParseError::Incomplete {
            need: total,
            have: input.len(),
        });
    }

    let methods = input[2..total].to_vec();
    Ok((methods, total))
}

// ─── Auth ─────────────────────────────────────────────────────────────

/// Разбор user/pass auth (RFC 1929): `[ver, ulen, uname, plen, passwd]`.
pub fn parse_auth(input: &[u8]) -> Result<((String, String), usize), ParseError> {
    if input.len() < 2 {
        return Err(ParseError::Incomplete {
            need: 2,
            have: input.len(),
        });
    }

    let version = input[0];
    let ulen = input[1] as usize;

    if version != AUTH_VERSION {
        return Err(ParseError::BadAuthVersion(version));
    }
    if ulen == 0 {
        return Err(ParseError::EmptyUsername);
    }

    let after_uname = 2 + ulen;
    if input.len() < after_uname + 1 {
        return Err(ParseError::Incomplete {
            need: after_uname + 1,
            have: input.len(),
        });
    }

    let plen = input[after_uname] as usize;
    if plen == 0 {
        return Err(ParseError::EmptyPassword);
    }

    let total = after_uname + 1 + plen;
    if input.len() < total {
        return Err(ParseError::Incomplete {
            need: total,
            have: input.len(),
        });
    }

    let uname = std::str::from_utf8(&input[2..after_uname])
        .map_err(|_| ParseError::InvalidUtf8("username"))?
        .to_string();

    let passwd = std::str::from_utf8(&input[after_uname + 1..total])
        .map_err(|_| ParseError::InvalidUtf8("password"))?
        .to_string();

    Ok(((uname, passwd), total))
}

// ─── Request ──────────────────────────────────────────────────────────

/// Разбор запроса: `[ver, cmd, rsv, atyp, addr..., port]`.
pub fn parse_request(input: &[u8]) -> Result<(Socks5Request, usize), ParseError> {
    if input.len() < 4 {
        return Err(ParseError::Incomplete {
            need: 4,
            have: input.len(),
        });
    }

    let version = input[0];
    let cmd = input[1];
    let rsv = input[2];
    let atyp = input[3];

    if version != SOCKS5_VERSION {
        return Err(ParseError::BadVersion(version));
    }
    if rsv != 0 {
        return Err(ParseError::NonZeroRsv(rsv));
    }
    if cmd != CMD_CONNECT && cmd != CMD_BIND && cmd != CMD_UDP_ASSOCIATE {
        return Err(ParseError::UnsupportedCommand(cmd));
    }

    let (target, consumed) = parse_target(input, atyp, 4)?;
    Ok((
        Socks5Request {
            command: cmd,
            target,
        },
        consumed,
    ))
}

fn parse_target(input: &[u8], atyp: u8, offset: usize) -> Result<(Endpoint, usize), ParseError> {
    match atyp {
        ATYP_IPV4 => {
            let total = offset + 4 + 2;
            if input.len() < total {
                return Err(ParseError::Incomplete {
                    need: total,
                    have: input.len(),
                });
            }
            let ip = Ipv4Addr::new(
                input[offset],
                input[offset + 1],
                input[offset + 2],
                input[offset + 3],
            );
            let port = u16::from_be_bytes([input[offset + 4], input[offset + 5]]);
            Ok((Endpoint::Ip(SocketAddr::new(IpAddr::V4(ip), port)), total))
        }

        ATYP_IPV6 => {
            let total = offset + 16 + 2;
            if input.len() < total {
                return Err(ParseError::Incomplete {
                    need: total,
                    have: input.len(),
                });
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&input[offset..offset + 16]);
            let ip = Ipv6Addr::from(octets);
            let port = u16::from_be_bytes([input[offset + 16], input[offset + 17]]);
            Ok((Endpoint::Ip(SocketAddr::new(IpAddr::V6(ip), port)), total))
        }

        ATYP_DOMAIN => {
            if input.len() < offset + 1 {
                return Err(ParseError::Incomplete {
                    need: offset + 1,
                    have: input.len(),
                });
            }
            let dlen = input[offset] as usize;
            if dlen == 0 {
                return Err(ParseError::EmptyDomain);
            }

            let after_domain = offset + 1 + dlen;
            let total = after_domain + 2;
            if input.len() < total {
                return Err(ParseError::Incomplete {
                    need: total,
                    have: input.len(),
                });
            }

            let domain = std::str::from_utf8(&input[offset + 1..after_domain])
                .map_err(|_| ParseError::InvalidUtf8("domain"))?
                .to_string();

            let port = u16::from_be_bytes([input[after_domain], input[after_domain + 1]]);
            Ok((Endpoint::Domain(domain, port), total))
        }

        other => Err(ParseError::UnsupportedAddressType(other)),
    }
}

/// Разбор UDP-заголовка SOCKS5 (RFC 1928): RSV + FRAG + ATYP + DST + payload.
pub fn parse_udp_datagram(input: &[u8]) -> Result<(Endpoint, usize), ParseError> {
    if input.len() < 4 {
        return Err(ParseError::Incomplete {
            need: 4,
            have: input.len(),
        });
    }
    if input[0] != 0 || input[1] != 0 {
        return Err(ParseError::NonZeroRsv(input[0]));
    }
    if input[2] != 0 {
        return Err(ParseError::UnsupportedCommand(input[2]));
    }
    let (target, header_end) = parse_target(input, input[3], 4)?;
    Ok((target, header_end))
}

/// Собрать UDP-заголовок SOCKS5 для ответа клиенту.
pub fn encode_udp_datagram(target: &Endpoint, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(10 + payload.len());
    buf.extend_from_slice(&[0, 0, 0]);
    match target {
        Endpoint::Ip(addr) => match addr {
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
        },
        Endpoint::Domain(host, port) => {
            let host_bytes = host.as_bytes();
            let dlen = host_bytes.len().min(255);
            buf.push(ATYP_DOMAIN);
            buf.push(dlen as u8);
            buf.extend_from_slice(&host_bytes[..dlen]);
            buf.extend_from_slice(&port.to_be_bytes());
        }
    }
    buf.extend_from_slice(payload);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_greeting_ok() {
        let input = [0x05, 0x02, 0x00, 0x02];
        let (methods, used) = parse_greeting(&input).unwrap();
        assert_eq!(methods, vec![0x00, 0x02]);
        assert_eq!(used, 4);
    }

    #[test]
    fn parse_greeting_incomplete() {
        assert!(matches!(
            parse_greeting(&[0x05]),
            Err(ParseError::Incomplete { .. })
        ));
    }

    #[test]
    fn parse_greeting_bad_version() {
        assert!(matches!(
            parse_greeting(&[0x04, 0x01, 0x00]),
            Err(ParseError::BadVersion(0x04))
        ));
    }

    #[test]
    fn parse_greeting_zero_methods() {
        assert!(matches!(
            parse_greeting(&[0x05, 0x00]),
            Err(ParseError::ZeroMethods)
        ));
    }

    #[test]
    fn parse_greeting_too_many() {
        let mut input = vec![0x05, 0xFF];
        input.extend(std::iter::repeat_n(0x00, 255));
        assert!(matches!(
            parse_greeting(&input),
            Err(ParseError::TooManyMethods(255))
        ));
    }

    #[test]
    fn parse_auth_ok() {
        let input = [0x01, 0x03, b'b', b'o', b'b', 0x04, b'p', b'a', b's', b's'];
        let ((u, p), used) = parse_auth(&input).unwrap();
        assert_eq!(u, "bob");
        assert_eq!(p, "pass");
        assert_eq!(used, 10);
    }

    #[test]
    fn parse_auth_empty_username() {
        assert!(matches!(
            parse_auth(&[0x01, 0x00]),
            Err(ParseError::EmptyUsername)
        ));
    }

    #[test]
    fn parse_auth_empty_password() {
        let input = [0x01, 0x01, b'a', 0x00];
        assert!(matches!(parse_auth(&input), Err(ParseError::EmptyPassword)));
    }

    #[test]
    fn parse_request_ipv4_ok() {
        let input = [0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0x1F, 0x90];
        let (req, used) = parse_request(&input).unwrap();
        assert_eq!(used, 10);
        assert_eq!(req.command, CMD_CONNECT);
        match req.target {
            Endpoint::Ip(addr) => {
                assert_eq!(addr.port(), 8080);
                assert_eq!(addr.ip().to_string(), "127.0.0.1");
            }
            _ => panic!("expected Ip"),
        }
    }

    #[test]
    fn parse_request_domain_ok() {
        let mut input = vec![0x05, 0x01, 0x00, 0x03, 11];
        input.extend_from_slice(b"example.com");
        input.extend_from_slice(&443u16.to_be_bytes());
        let (req, used) = parse_request(&input).unwrap();
        assert_eq!(used, 4 + 1 + 11 + 2);
        match req.target {
            Endpoint::Domain(d, p) => {
                assert_eq!(d, "example.com");
                assert_eq!(p, 443);
            }
            _ => panic!("expected Domain"),
        }
    }

    #[test]
    fn parse_request_bind_ok() {
        let input = [0x05, 0x02, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
        let (req, _) = parse_request(&input).unwrap();
        assert_eq!(req.command, CMD_BIND);
    }

    #[test]
    fn parse_request_unsupported_command() {
        let input = [0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
        assert!(matches!(
            parse_request(&input),
            Err(ParseError::UnsupportedCommand(0x04))
        ));
    }

    #[test]
    fn parse_request_bad_atyp() {
        let input = [0x05, 0x01, 0x00, 0xFF, 0, 0, 0, 0, 0, 0];
        assert!(matches!(
            parse_request(&input),
            Err(ParseError::UnsupportedAddressType(0xFF))
        ));
    }
}
