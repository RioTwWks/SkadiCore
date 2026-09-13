//! Inbound application protocols (SOCKS5 / VLESS).

use crate::error::{Error, Result};

/// Первый байт SOCKS5 greeting (RFC 1928).
pub const SOCKS5_WIRE_BYTE: u8 = 0x05;
/// Версия VLESS v0.
pub const VLESS_WIRE_BYTE: u8 = 0x00;

/// Прикладной протокол поверх TCP/TLS/REALITY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    Socks5,
    Vless,
}

impl Protocol {
    /// Первый байт на wire для sniffing.
    pub fn wire_byte(self) -> u8 {
        match self {
            Self::Socks5 => SOCKS5_WIRE_BYTE,
            Self::Vless => VLESS_WIRE_BYTE,
        }
    }

    /// Распознать протокол по первому байту без проверки enabled-флагов.
    pub fn from_wire_byte(byte: u8) -> Option<Self> {
        match byte {
            SOCKS5_WIRE_BYTE => Some(Self::Socks5),
            VLESS_WIRE_BYTE => Some(Self::Vless),
            _ => None,
        }
    }

    /// Базовая метка для логов/метрик (без VLESS command).
    pub fn metric_label(self) -> &'static str {
        match self {
            Self::Socks5 => "socks5",
            Self::Vless => "vless",
        }
    }
}

/// Какие inbound-протоколы включены в конфиге.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnabledProtocols {
    pub socks5: bool,
    pub vless: bool,
}

impl EnabledProtocols {
    pub fn count(self) -> usize {
        usize::from(self.socks5) + usize::from(self.vless)
    }

    /// Нужен sniffing первого байта (оба протокола включены).
    pub fn needs_sniffing(self) -> bool {
        self.count() > 1
    }

    /// Единственный включённый протокол (если ровно один).
    pub fn sole(self) -> Option<Protocol> {
        match (self.socks5, self.vless) {
            (true, false) => Some(Protocol::Socks5),
            (false, true) => Some(Protocol::Vless),
            _ => None,
        }
    }

    /// Распознать протокол по первому байту с учётом enabled-флагов.
    pub fn detect(self, byte: u8) -> Result<Protocol> {
        match Protocol::from_wire_byte(byte) {
            Some(Protocol::Socks5) if self.socks5 => Ok(Protocol::Socks5),
            Some(Protocol::Vless) if self.vless => Ok(Protocol::Vless),
            Some(Protocol::Socks5) => Err(Error::DisabledProtocol("socks5")),
            Some(Protocol::Vless) => Err(Error::DisabledProtocol("vless")),
            None => Err(Error::UnknownProtocolByte(byte)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_bytes_roundtrip() {
        assert_eq!(Protocol::Socks5.wire_byte(), SOCKS5_WIRE_BYTE);
        assert_eq!(Protocol::Vless.wire_byte(), VLESS_WIRE_BYTE);
        assert_eq!(Protocol::from_wire_byte(0x05), Some(Protocol::Socks5));
        assert_eq!(Protocol::from_wire_byte(0x00), Some(Protocol::Vless));
        assert_eq!(Protocol::from_wire_byte(0xFF), None);
    }

    #[test]
    fn enabled_protocols_sniffing() {
        let both = EnabledProtocols {
            socks5: true,
            vless: true,
        };
        assert!(both.needs_sniffing());
        assert_eq!(both.sole(), None);

        let vless_only = EnabledProtocols {
            socks5: false,
            vless: true,
        };
        assert!(!vless_only.needs_sniffing());
        assert_eq!(vless_only.sole(), Some(Protocol::Vless));
    }

    #[test]
    fn detect_respects_enabled_flags() {
        let both = EnabledProtocols {
            socks5: true,
            vless: true,
        };
        assert_eq!(both.detect(0x05).unwrap(), Protocol::Socks5);
        assert_eq!(both.detect(0x00).unwrap(), Protocol::Vless);

        let socks_only = EnabledProtocols {
            socks5: true,
            vless: false,
        };
        assert!(socks_only.detect(0x00).is_err());

        let vless_only = EnabledProtocols {
            socks5: false,
            vless: true,
        };
        assert!(vless_only.detect(0x05).is_err());
    }
}
