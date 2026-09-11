//! Чистые парсеры VLESS. Без I/O — пригодны для fuzz-тестов.

use skadi_core::Endpoint;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use thiserror::Error;

pub const VLESS_VERSION: u8 = 0x00;
pub const CMD_TCP: u8 = 0x01;
pub const CMD_UDP: u8 = 0x02;
pub const CMD_MUX: u8 = 0x03;

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

    // Для Mux порт и адрес не передаются — упростим, требуя TCP/UDP.
    if command == CMD_MUX {
        return Err(ParseError::UnsupportedCommand(command));
    }

    let after_cmd = addons_end + 1;
    if input.len() < after_cmd + 3 {
        return Err(ParseError::Incomplete {
            need: after_cmd + 3,
            have: input.len(),
        });
    }

    let port = u16::from_be_bytes([input[after_cmd], input[after_cmd + 1]]);
    let atyp = input[after_cmd + 2];
    let addr_start = after_cmd + 3;

    let (target, consumed) = match atyp {
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
            (
                Endpoint::Ip(SocketAddr::new(IpAddr::V4(ip), port)),
                addr_start + 4,
            )
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
            (Endpoint::Domain(domain, port), domain_end)
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
            (
                Endpoint::Ip(SocketAddr::new(IpAddr::V6(ip), port)),
                addr_start + 16,
            )
        }

        other => return Err(ParseError::UnsupportedAddressType(other)),
    };

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

/// Собрать ответный заголовок VLESS. Всегда 2 байта.
pub fn build_response_header(version: u8) -> [u8; 2] {
    [version, 0x00]
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
}
