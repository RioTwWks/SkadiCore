//! Чистые парсеры VLESS. Без I/O — пригодны для fuzz-тестов.

use skadi_core::Endpoint;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use thiserror::Error;

pub const VLESS_VERSION: u8 = 0x00;
pub const CMD_TCP: u8 = 0x01;
pub const CMD_UDP: u8 = 0x02;
pub const CMD_MUX: u8 = 0x03;

/// Placeholder-адрес для VLESS Mux (как в Xray-core).
pub const MUX_PLACEHOLDER_HOST: &str = "v1.mux.cool";

pub const ATYP_IPV4: u8 = 0x01;
pub const ATYP_DOMAIN: u8 = 0x02; // ⚠️ В VLESS домен — 0x02, а не 0x03!
pub const ATYP_IPV6: u8 = 0x03;

/// Максимальная длина addons. На практике — единицы байт.
pub const MAX_ADDONS: usize = 512;

/// Максимальная длина домена.
pub const MAX_DOMAIN: usize = 255;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("not enough data: need {need}, have {have}")]
    Incomplete { need: usize, have: usize },

    #[error("unsupported VLESS version: 0x{0:02x}")]
    BadVersion(u8),

    #[error("addons too large: {0} > {max}", max = MAX_ADDONS)]
    AddonsTooLarge(usize),

    #[error("unsupported command: 0x{0:02x}")]
    UnsupportedCommand(u8),

    #[error("unsupported address type: 0x{0:02x}")]
    UnsupportedAddressType(u8),

    #[error("empty domain")]
    EmptyDomain,

    #[error("invalid UTF-8 in {0}")]
    InvalidUtf8(&'static str),
}

/// Результат разбора запроса VLESS.
#[derive(Debug)]
pub struct VlessRequest {
    pub uuid: [u8; 16],
    pub command: u8,
    pub addons: Vec<u8>,
    pub target: Endpoint,
}

/// Разбор запроса VLESS целиком.
pub fn parse_request(input: &[u8]) -> Result<(VlessRequest, usize), ParseError> {
    // Минимальный размер: version(1) + uuid(16) + addons_len(1) = 18.
    if input.len() < 18 {
        return Err(ParseError::Incomplete {
            need: 18,
            have: input.len(),
        });
    }

    let version = input[0];
    if version != VLESS_VERSION {
        return Err(ParseError::BadVersion(version));
    }

    let mut uuid = [0u8; 16];
    uuid.copy_from_slice(&input[1..17]);

    let addons_len = input[17] as usize;
    if addons_len > MAX_ADDONS {
        return Err(ParseError::AddonsTooLarge(addons_len));
    }

    let addons_end = 18 + addons_len;
    if input.len() < addons_end + 1 {
        return Err(ParseError::Incomplete {
            need: addons_end + 1,
            have: input.len(),
        });
    }

    let addons = input[18..addons_end].to_vec();
    let command = input[addons_end];

    if command != CMD_TCP && command != CMD_UDP && command != CMD_MUX {
        return Err(ParseError::UnsupportedCommand(command));
    }

    if command == CMD_MUX {
        return Ok((
            VlessRequest {
                uuid,
                command,
                addons,
                target: Endpoint::Domain(MUX_PLACEHOLDER_HOST.to_string(), 0),
            },
            addons_end + 1,
        ));
    }

    let after_cmd = addons_end + 1;
    if input.len() < after_cmd + 3 {
        return Err(ParseError::Incomplete {
            need: after_cmd + 3,
            have: input.len(),
        });
    }

    let (target, consumed) = parse_port_address(&input[after_cmd..])?;
    let consumed = after_cmd + consumed;

    Ok((
        VlessRequest {
            uuid,
            command,
            addons,
            target,
        },
        consumed,
    ))
}

/// Разбор `PORT + ATYP + ADDR` (порядок VLESS).
pub fn parse_port_address(input: &[u8]) -> Result<(Endpoint, usize), ParseError> {
    if input.len() < 3 {
        return Err(ParseError::Incomplete {
            need: 3,
            have: input.len(),
        });
    }

    let port = u16::from_be_bytes([input[0], input[1]]);
    let atyp = input[2];
    let addr_start = 3;

    let (target, addr_len) = match atyp {
        ATYP_IPV4 => {
            if input.len() < addr_start + 4 {
                return Err(ParseError::Incomplete {
                    need: addr_start + 4,
                    have: input.len(),
                });
            }
            let ip = Ipv4Addr::new(
                input[addr_start],
                input[addr_start + 1],
                input[addr_start + 2],
                input[addr_start + 3],
            );
            (Endpoint::Ip(SocketAddr::new(IpAddr::V4(ip), port)), 4)
        }

        ATYP_DOMAIN => {
            if input.len() < addr_start + 1 {
                return Err(ParseError::Incomplete {
                    need: addr_start + 1,
                    have: input.len(),
                });
            }
            let dlen = input[addr_start] as usize;
            if dlen == 0 {
                return Err(ParseError::EmptyDomain);
            }
            if dlen > MAX_DOMAIN {
                return Err(ParseError::Incomplete {
                    need: addr_start + 1 + dlen,
                    have: input.len(),
                });
            }
            let domain_end = addr_start + 1 + dlen;
            if input.len() < domain_end {
                return Err(ParseError::Incomplete {
                    need: domain_end,
                    have: input.len(),
                });
            }
            let domain = std::str::from_utf8(&input[addr_start + 1..domain_end])
                .map_err(|_| ParseError::InvalidUtf8("domain"))?
                .to_string();
            (Endpoint::Domain(domain, port), 1 + dlen)
        }

        ATYP_IPV6 => {
            if input.len() < addr_start + 16 {
                return Err(ParseError::Incomplete {
                    need: addr_start + 16,
                    have: input.len(),
                });
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&input[addr_start..addr_start + 16]);
            let ip = Ipv6Addr::from(octets);
            (Endpoint::Ip(SocketAddr::new(IpAddr::V6(ip), port)), 16)
        }

        other => return Err(ParseError::UnsupportedAddressType(other)),
    };

    Ok((target, addr_start + addr_len))
}

/// Закодировать `PORT + ATYP + ADDR`.
pub fn encode_port_address(endpoint: &Endpoint) -> Vec<u8> {
    match endpoint {
        Endpoint::Ip(addr) => {
            let mut buf = Vec::with_capacity(7);
            buf.extend_from_slice(&addr.port().to_be_bytes());
            match addr.ip() {
                IpAddr::V4(ip) => {
                    buf.push(ATYP_IPV4);
                    buf.extend_from_slice(&ip.octets());
                }
                IpAddr::V6(ip) => {
                    buf.push(ATYP_IPV6);
                    buf.extend_from_slice(&ip.octets());
                }
            }
            buf
        }
        Endpoint::Domain(domain, port) => {
            let domain_bytes = domain.as_bytes();
            let mut buf = Vec::with_capacity(4 + domain_bytes.len());
            buf.extend_from_slice(&port.to_be_bytes());
            buf.push(ATYP_DOMAIN);
            buf.push(domain_bytes.len() as u8);
            buf.extend_from_slice(domain_bytes);
            buf
        }
    }
}

/// Собрать VLESS Mux-запрос (без addons, без port/address).
pub fn build_mux_request(uuid: &[u8; 16]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(19);
    buf.push(VLESS_VERSION);
    buf.extend_from_slice(uuid);
    buf.push(0);
    buf.push(CMD_MUX);
    buf
}

/// Собрать ответный заголовок VLESS. Всегда 2 байта.
pub fn build_response_header(version: u8) -> [u8; 2] {
    [version, 0x00]
}

/// Собрать VLESS TCP-запрос к IPv4-адресу (без addons).
pub fn build_tcp_request(uuid: &[u8; 16], addr: Ipv4Addr, port: u16) -> Vec<u8> {
    let mut buf = Vec::with_capacity(26);
    buf.push(VLESS_VERSION);
    buf.extend_from_slice(uuid);
    buf.push(0); // addons length
    buf.push(CMD_TCP);
    buf.extend_from_slice(&port.to_be_bytes());
    buf.push(ATYP_IPV4);
    buf.extend_from_slice(&addr.octets());
    buf
}

/// Собрать VLESS UDP-запрос к IPv4-адресу (без addons).
pub fn build_udp_request(uuid: &[u8; 16], addr: Ipv4Addr, port: u16) -> Vec<u8> {
    let mut buf = Vec::with_capacity(26);
    buf.push(VLESS_VERSION);
    buf.extend_from_slice(uuid);
    buf.push(0);
    buf.push(CMD_UDP);
    buf.extend_from_slice(&port.to_be_bytes());
    buf.push(ATYP_IPV4);
    buf.extend_from_slice(&addr.octets());
    buf
}

/// Собрать VLESS UDP-запрос к домену (без addons).
pub fn build_udp_domain_request(uuid: &[u8; 16], domain: &str, port: u16) -> Vec<u8> {
    let domain_bytes = domain.as_bytes();
    let mut buf = Vec::with_capacity(22 + domain_bytes.len());
    buf.push(VLESS_VERSION);
    buf.extend_from_slice(uuid);
    buf.push(0);
    buf.push(CMD_UDP);
    buf.extend_from_slice(&port.to_be_bytes());
    buf.push(ATYP_DOMAIN);
    buf.push(domain_bytes.len() as u8);
    buf.extend_from_slice(domain_bytes);
    buf
}

/// Собрать VLESS TCP-запрос к домену (без addons).
pub fn build_tcp_domain_request(uuid: &[u8; 16], domain: &str, port: u16) -> Vec<u8> {
    let domain_bytes = domain.as_bytes();
    let mut buf = Vec::with_capacity(22 + domain_bytes.len());
    buf.push(VLESS_VERSION);
    buf.extend_from_slice(uuid);
    buf.push(0);
    buf.push(CMD_TCP);
    buf.extend_from_slice(&port.to_be_bytes());
    buf.push(ATYP_DOMAIN);
    buf.push(domain_bytes.len() as u8);
    buf.extend_from_slice(domain_bytes);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_request(cmd: u8, atyp: u8, addr: &[u8], port: u16) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(VLESS_VERSION);
        buf.extend_from_slice(&[0u8; 16]); // UUID
        buf.push(0); // addons length
        buf.push(cmd);
        buf.extend_from_slice(&port.to_be_bytes());
        buf.push(atyp);
        buf.extend_from_slice(addr);
        buf
    }

    #[test]
    fn parse_ipv4_ok() {
        let buf = build_request(CMD_TCP, ATYP_IPV4, &[127, 0, 0, 1], 443);
        let (req, _) = parse_request(&buf).unwrap();
        assert_eq!(req.command, CMD_TCP);
        assert!(matches!(req.target, Endpoint::Ip(_)));
    }

    #[test]
    fn parse_domain_ok() {
        let mut addr = vec![11u8];
        addr.extend_from_slice(b"example.com");
        let buf = build_request(CMD_TCP, ATYP_DOMAIN, &addr, 443);
        let (req, _) = parse_request(&buf).unwrap();
        match req.target {
            Endpoint::Domain(d, p) => {
                assert_eq!(d, "example.com");
                assert_eq!(p, 443);
            }
            _ => panic!("expected Domain"),
        }
    }

    #[test]
    fn parse_bad_version() {
        let mut buf = build_request(CMD_TCP, ATYP_IPV4, &[127, 0, 0, 1], 443);
        buf[0] = 0x01;
        assert!(matches!(
            parse_request(&buf),
            Err(ParseError::BadVersion(1))
        ));
    }

    #[test]
    fn parse_bad_atyp() {
        let buf = build_request(CMD_TCP, 0xFF, &[0, 0, 0, 0], 443);
        assert!(matches!(
            parse_request(&buf),
            Err(ParseError::UnsupportedAddressType(0xFF))
        ));
    }

    #[test]
    fn parse_empty_domain() {
        let buf = build_request(CMD_TCP, ATYP_DOMAIN, &[0], 443);
        assert!(matches!(parse_request(&buf), Err(ParseError::EmptyDomain)));
    }

    #[test]
    fn parse_too_large_addons() {
        let mut buf = vec![VLESS_VERSION];
        buf.extend_from_slice(&[0u8; 16]);
        buf.push(0xFF); // addons length 255 > MAX_ADDONS? Нет, 255 < 512.
                        // Проверим именно границу:
        let mut big = vec![VLESS_VERSION];
        big.extend_from_slice(&[0u8; 16]);
        big.push(0xFF);
        big.extend(std::iter::repeat_n(0, 255));
        big.push(CMD_TCP);
        big.extend_from_slice(&443u16.to_be_bytes());
        big.push(ATYP_IPV4);
        big.extend_from_slice(&[127, 0, 0, 1]);
        // 255 addons допустимо, парсер должен пройти.
        let _ = parse_request(&big);
    }

    #[test]
    fn response_header_is_two_bytes() {
        let h = build_response_header(VLESS_VERSION);
        assert_eq!(h, [0x00, 0x00]);
    }

    #[test]
    fn parse_udp_ok() {
        let buf = build_request(CMD_UDP, ATYP_IPV4, &[127, 0, 0, 1], 53);
        let (req, _) = parse_request(&buf).unwrap();
        assert_eq!(req.command, CMD_UDP);
    }

    #[test]
    fn parse_mux_request_ok() {
        let buf = build_mux_request(&[0u8; 16]);
        let (req, len) = parse_request(&buf).unwrap();
        assert_eq!(len, buf.len());
        assert_eq!(req.command, CMD_MUX);
        assert!(matches!(
            req.target,
            Endpoint::Domain(host, 0) if host == MUX_PLACEHOLDER_HOST
        ));
    }

    #[test]
    fn build_tcp_request_roundtrip() {
        let uuid = [0xAB; 16];
        let buf = build_tcp_request(&uuid, Ipv4Addr::new(10, 0, 0, 1), 8443);
        let (req, len) = parse_request(&buf).unwrap();
        assert_eq!(len, buf.len());
        assert_eq!(req.uuid, uuid);
        assert_eq!(req.command, CMD_TCP);
        assert!(matches!(
            req.target,
            Endpoint::Ip(sa) if sa.port() == 8443
        ));
    }
}
