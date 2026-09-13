//! Конфигурация XHTTP (SplitHTTP) inbound.

/// Режим XHTTP (совместимость с Xray splithttp).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XhttpMode {
    Auto,
    PacketUp,
    StreamUp,
    StreamOne,
}

impl XhttpMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "" | "auto" => Some(Self::Auto),
            "packet-up" => Some(Self::PacketUp),
            "stream-up" => Some(Self::StreamUp),
            "stream-one" => Some(Self::StreamOne),
            _ => None,
        }
    }

    /// Сервер в режиме auto принимает stream-one (без session id).
    pub fn allows_stream_one(self) -> bool {
        matches!(self, Self::Auto | Self::StreamOne | Self::StreamUp)
    }

    /// Сервер принимает stream-up (GET/POST с session id).
    pub fn allows_stream_up(self) -> bool {
        matches!(self, Self::Auto | Self::StreamUp | Self::StreamOne)
    }
}

/// Диапазон длины X-Padding (байты).
#[derive(Debug, Clone, Copy)]
pub struct PaddingRange {
    pub min: u32,
    pub max: u32,
}

impl PaddingRange {
    pub fn new(min: u32, max: u32) -> Self {
        Self { min, max }
    }

    pub fn sample_len(&self) -> usize {
        if self.max <= self.min {
            return self.min as usize;
        }
        let span = self.max - self.min;
        self.min as usize + rand::random::<u32>() as usize % (span as usize + 1)
    }
}

impl Default for PaddingRange {
    fn default() -> Self {
        Self {
            min: 100,
            max: 1000,
        }
    }
}

/// Runtime-конфиг XHTTP inbound.
#[derive(Debug, Clone)]
pub struct XhttpConfig {
    pub path: String,
    pub host: Option<String>,
    pub mode: XhttpMode,
    pub padding: PaddingRange,
    pub no_sse_header: bool,
}

impl XhttpConfig {
    pub fn normalized_path(&self) -> String {
        let path = self.path.split('?').next().unwrap_or(&self.path);
        if path.is_empty() || !path.starts_with('/') {
            format!("/{}", path.trim_start_matches('/'))
        } else {
            path.to_string()
        }
    }

    pub fn host_matches(&self, request_host: &str) -> bool {
        match &self.host {
            Some(expected) if !expected.is_empty() => {
                request_host.eq_ignore_ascii_case(expected)
                    || request_host
                        .split(':')
                        .next()
                        .is_some_and(|h| h.eq_ignore_ascii_case(expected))
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parse() {
        assert_eq!(XhttpMode::parse("auto"), Some(XhttpMode::Auto));
        assert_eq!(XhttpMode::parse("stream-one"), Some(XhttpMode::StreamOne));
        assert!(XhttpMode::parse("invalid").is_none());
    }

    #[test]
    fn normalized_path() {
        let cfg = XhttpConfig {
            path: "xhttp".to_string(),
            host: None,
            mode: XhttpMode::Auto,
            padding: PaddingRange::default(),
            no_sse_header: false,
        };
        assert_eq!(cfg.normalized_path(), "/xhttp");
    }
}
