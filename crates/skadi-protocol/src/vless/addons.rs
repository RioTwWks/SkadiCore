//! Разбор VLESS addons (Protobuf) без внешних зависимостей.

use thiserror::Error;

/// Известный неподдерживаемый flow XTLS Vision.
pub const FLOW_XTLS_VISION: &str = "xtls-rprx-vision";

/// Результат разбора addons.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VlessAddons {
    pub flow: Option<String>,
    pub seed: Option<Vec<u8>>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AddonsError {
    #[error("truncated protobuf field")]
    Truncated,

    #[error("invalid protobuf wire type: {0}")]
    InvalidWireType(u8),

    #[error("nested flow message too large: {0}")]
    FlowTooLarge(usize),

    #[error("seed too large: {0}")]
    SeedTooLarge(usize),
}

const MAX_NESTED: usize = 128;
const MAX_SEED: usize = 64;

/// Разобрать поле `ADDONS` из VLESS-запроса.
pub fn parse_addons(data: &[u8]) -> Result<VlessAddons, AddonsError> {
    if data.is_empty() {
        return Ok(VlessAddons::default());
    }

    let mut addons = VlessAddons::default();
    let mut offset = 0;

    while offset < data.len() {
        let (tag, wire_type, next) = read_tag(data, offset)?;
        offset = next;

        match (tag, wire_type) {
            (1, 2) => {
                let (nested, next) = read_length_delimited(data, offset, MAX_NESTED)?;
                offset = next;
                addons.flow = Some(parse_flow_message(nested)?);
            }
            (2, 2) => {
                let (seed, next) = read_length_delimited(data, offset, MAX_SEED)?;
                offset = next;
                addons.seed = Some(seed.to_vec());
            }
            (_, 0) => {
                let next = skip_varint(data, offset)?;
                offset = next;
            }
            (_, 1) => offset = offset.saturating_add(8),
            (_, 2) => {
                let (_, next) = read_length_delimited(data, offset, MAX_ADDONS_SKIP)?;
                offset = next;
            }
            (_, 5) => offset = offset.saturating_add(4),
            (_, other) => return Err(AddonsError::InvalidWireType(other)),
        }
    }

    Ok(addons)
}

const MAX_ADDONS_SKIP: usize = 512;

/// Проверить, что flow поддерживается сервером.
pub fn validate_flow(flow: Option<&str>, user_flow: Option<&str>) -> Result<(), UnsupportedFlow> {
    if let Some(flow) = flow {
        if flow == FLOW_XTLS_VISION {
            return Err(UnsupportedFlow::Vision);
        }
        if !flow.is_empty() {
            return Err(UnsupportedFlow::Unknown(flow.to_string()));
        }
    }

    if let Some(expected) = user_flow {
        if !expected.is_empty() && flow != Some(expected) {
            return Err(UnsupportedFlow::Mismatch {
                expected: expected.to_string(),
                got: flow.map(str::to_string),
            });
        }
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum UnsupportedFlow {
    Vision,
    Unknown(String),
    Mismatch {
        expected: String,
        got: Option<String>,
    },
}

fn parse_flow_message(data: &[u8]) -> Result<String, AddonsError> {
    let mut flow_type = String::new();
    let mut offset = 0;

    while offset < data.len() {
        let (tag, wire_type, next) = read_tag(data, offset)?;
        offset = next;

        match (tag, wire_type) {
            (1, 2) => {
                let (value, next) = read_length_delimited(data, offset, MAX_NESTED)?;
                offset = next;
                flow_type = std::str::from_utf8(value)
                    .map_err(|_| AddonsError::Truncated)?
                    .to_string();
            }
            (_, 0) => {
                offset = skip_varint(data, offset)?;
            }
            (_, 1) => offset = offset.saturating_add(8),
            (_, 2) => {
                let (_, next) = read_length_delimited(data, offset, MAX_NESTED)?;
                offset = next;
            }
            (_, 5) => offset = offset.saturating_add(4),
            (_, other) => return Err(AddonsError::InvalidWireType(other)),
        }
    }

    Ok(flow_type)
}

fn read_tag(data: &[u8], offset: usize) -> Result<(u32, u8, usize), AddonsError> {
    let (value, next) = read_varint(data, offset)?;
    let wire_type = (value & 0x07) as u8;
    let tag = (value >> 3) as u32;
    Ok((tag, wire_type, next))
}

fn read_varint(data: &[u8], offset: usize) -> Result<(u64, usize), AddonsError> {
    let mut result = 0u64;
    let mut shift = 0;
    let mut pos = offset;

    while pos < data.len() {
        let byte = data[pos];
        pos += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok((result, pos));
        }
        shift += 7;
        if shift > 63 {
            return Err(AddonsError::Truncated);
        }
    }

    Err(AddonsError::Truncated)
}

fn skip_varint(data: &[u8], offset: usize) -> Result<usize, AddonsError> {
    read_varint(data, offset).map(|(_, next)| next)
}

fn read_length_delimited(
    data: &[u8],
    offset: usize,
    max_len: usize,
) -> Result<(&[u8], usize), AddonsError> {
    let (len, next) = read_varint(data, offset)?;
    let len = len as usize;
    if len > max_len {
        return Err(if max_len == MAX_SEED {
            AddonsError::SeedTooLarge(len)
        } else {
            AddonsError::FlowTooLarge(len)
        });
    }
    let end = next + len;
    if end > data.len() {
        return Err(AddonsError::Truncated);
    }
    Ok((&data[next..end], end))
}

/// Собрать addons с flow (для тестов).
pub fn build_addons_with_flow(flow: &str) -> Vec<u8> {
    let flow_msg = build_flow_message(flow);
    let mut out = Vec::with_capacity(2 + flow_msg.len());
    out.push(0x0A); // field 1, wire type 2
    out.push(flow_msg.len() as u8);
    out.extend_from_slice(&flow_msg);
    out
}

fn build_flow_message(flow: &str) -> Vec<u8> {
    let flow_bytes = flow.as_bytes();
    let mut out = Vec::with_capacity(2 + flow_bytes.len());
    out.push(0x0A); // field 1 (type), wire type 2
    out.push(flow_bytes.len() as u8);
    out.extend_from_slice(flow_bytes);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_addons() {
        let addons = parse_addons(&[]).unwrap();
        assert_eq!(addons, VlessAddons::default());
    }

    #[test]
    fn parse_vision_flow() {
        let raw = build_addons_with_flow(FLOW_XTLS_VISION);
        let addons = parse_addons(&raw).unwrap();
        assert_eq!(addons.flow.as_deref(), Some(FLOW_XTLS_VISION));
        assert!(validate_flow(addons.flow.as_deref(), None).is_err());
    }

    #[test]
    fn reject_vision() {
        assert_eq!(
            validate_flow(Some(FLOW_XTLS_VISION), None),
            Err(UnsupportedFlow::Vision)
        );
    }

    #[test]
    fn flow_mismatch() {
        assert_eq!(
            validate_flow(None, Some("xtls-rprx-vision")),
            Err(UnsupportedFlow::Mismatch {
                expected: "xtls-rprx-vision".into(),
                got: None,
            })
        );
    }
}
