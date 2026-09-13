//! Xray-compatible Mux frame metadata (common/mux/frame.go).

use skadi_core::Endpoint;
use thiserror::Error;

use super::super::parse::{encode_port_address, parse_port_address};

pub const SESSION_STATUS_NEW: u8 = 0x01;
pub const SESSION_STATUS_KEEP: u8 = 0x02;
pub const SESSION_STATUS_END: u8 = 0x03;
pub const SESSION_STATUS_KEEP_ALIVE: u8 = 0x04;

pub const OPTION_DATA: u8 = 0x01;
pub const OPTION_ERROR: u8 = 0x02;

pub const NETWORK_TCP: u8 = 0x01;
pub const NETWORK_UDP: u8 = 0x02;

pub const MAX_META_LEN: usize = 512;
pub const MAX_CHUNK_SIZE: usize = 8 * 1024;

/// XUDP всегда использует session ID 0 (Mux.Cool / Xray).
pub const XUDP_SESSION_ID: u16 = 0;
pub const GLOBAL_ID_LEN: usize = 8;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MuxError {
    #[error("mux metadata too short")]
    TooShort,

    #[error("mux metadata too large: {0}")]
    MetaTooLarge(usize),

    #[error("mux chunk too large: {0}")]
    ChunkTooLarge(usize),

    #[error("unknown mux network: 0x{0:02x}")]
    UnknownNetwork(u8),

    #[error("unknown mux session status: 0x{0:02x}")]
    UnknownStatus(u8),

    #[error("address parse error: {0}")]
    Address(String),
}

/// Метаданные одного Mux-кадра (без внешнего 2-byte length prefix).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuxMeta {
    pub session_id: u16,
    pub status: u8,
    pub option: u8,
    pub network: Option<u8>,
    pub target: Option<Endpoint>,
    /// 8-byte GlobalID (только в первом XUDP New-кадре).
    pub global_id: Option<[u8; 8]>,
}

impl MuxMeta {
    pub fn has_data(&self) -> bool {
        self.option & OPTION_DATA != 0
    }
}

/// Полный Mux-кадр: метаданные + опциональный payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuxFrame {
    pub meta: MuxMeta,
    pub payload: Option<Vec<u8>>,
}

/// Разобрать тело метаданных (после чтения `meta_len` байт).
pub fn parse_meta_body(body: &[u8]) -> Result<MuxMeta, MuxError> {
    if body.len() < 4 {
        return Err(MuxError::TooShort);
    }

    let session_id = u16::from_be_bytes([body[0], body[1]]);
    let status = body[2];
    let option = body[3];
    let mut offset = 4usize;
    let mut network = None;
    let mut target = None;
    let mut global_id = None;

    let needs_target = body.len() > offset
        && ((status == SESSION_STATUS_NEW
            && (body[offset] == NETWORK_TCP || body[offset] == NETWORK_UDP))
            || (status == SESSION_STATUS_KEEP && body[offset] == NETWORK_UDP));

    if needs_target {
        network = Some(body[offset]);
        offset += 1;

        let (endpoint, consumed) =
            parse_port_address(&body[offset..]).map_err(|e| MuxError::Address(e.to_string()))?;
        target = Some(endpoint);
        offset += consumed;

        if status == SESSION_STATUS_NEW
            && network == Some(NETWORK_UDP)
            && body.len() >= offset + GLOBAL_ID_LEN
        {
            global_id = Some(body[offset..offset + GLOBAL_ID_LEN].try_into().unwrap());
        }
    }

    Ok(MuxMeta {
        session_id,
        status,
        option,
        network,
        target,
        global_id,
    })
}

/// Закодировать метаданные с внешним 2-byte length prefix.
pub fn encode_meta(meta: &MuxMeta) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&meta.session_id.to_be_bytes());
    body.push(meta.status);
    body.push(meta.option);

    match meta.status {
        SESSION_STATUS_NEW => {
            if let (Some(network), Some(target)) = (meta.network, &meta.target) {
                body.push(network);
                body.extend_from_slice(&encode_port_address(target));
                if network == NETWORK_UDP {
                    if let Some(id) = meta.global_id {
                        body.extend_from_slice(&id);
                    }
                }
            }
        }
        SESSION_STATUS_KEEP => {
            if let (Some(NETWORK_UDP), Some(target)) = (meta.network, &meta.target) {
                body.push(NETWORK_UDP);
                body.extend_from_slice(&encode_port_address(target));
            }
        }
        _ => {}
    }

    let meta_len = body.len();
    let mut out = Vec::with_capacity(2 + meta_len);
    out.extend_from_slice(&(meta_len as u16).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

/// Закодировать кадр с данными (`OptionData` + 2-byte chunk length).
pub fn encode_data_frame(meta: &MuxMeta, payload: &[u8]) -> Result<Vec<u8>, MuxError> {
    if payload.len() > MAX_CHUNK_SIZE {
        return Err(MuxError::ChunkTooLarge(payload.len()));
    }

    let mut frame = encode_meta(meta);
    frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Закодировать End-кадр для сессии.
pub fn encode_end_frame(session_id: u16, with_error: bool) -> Vec<u8> {
    let option = if with_error { OPTION_ERROR } else { 0 };
    encode_meta(&MuxMeta {
        session_id,
        status: SESSION_STATUS_END,
        option,
        network: None,
        target: None,
        global_id: None,
    })
}

/// Разобрать полный кадр из буфера (meta length + body + optional chunk).
pub fn parse_frame(input: &[u8]) -> Result<(MuxFrame, usize), MuxError> {
    if input.len() < 2 {
        return Err(MuxError::TooShort);
    }

    let meta_len = u16::from_be_bytes([input[0], input[1]]) as usize;
    if meta_len > MAX_META_LEN {
        return Err(MuxError::MetaTooLarge(meta_len));
    }

    let meta_end = 2 + meta_len;
    if input.len() < meta_end {
        return Err(MuxError::TooShort);
    }

    let meta = parse_meta_body(&input[2..meta_end])?;
    let mut consumed = meta_end;

    let payload = if meta.has_data() {
        if input.len() < consumed + 2 {
            return Err(MuxError::TooShort);
        }
        let chunk_len = u16::from_be_bytes([input[consumed], input[consumed + 1]]) as usize;
        consumed += 2;
        if chunk_len > MAX_CHUNK_SIZE {
            return Err(MuxError::ChunkTooLarge(chunk_len));
        }
        if input.len() < consumed + chunk_len {
            return Err(MuxError::TooShort);
        }
        let data = input[consumed..consumed + chunk_len].to_vec();
        consumed += chunk_len;
        Some(data)
    } else {
        None
    };

    Ok((MuxFrame { meta, payload }, consumed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};

    #[test]
    fn new_tcp_meta_roundtrip() {
        let target = Endpoint::Ip(SocketAddr::new(Ipv4Addr::new(10, 0, 0, 1).into(), 443));
        let meta = MuxMeta {
            session_id: 42,
            status: SESSION_STATUS_NEW,
            option: OPTION_DATA,
            network: Some(NETWORK_TCP),
            target: Some(target.clone()),
            global_id: None,
        };

        let encoded = encode_meta(&meta);
        let (frame, _) = parse_frame(&{
            let mut buf = encoded;
            buf.extend_from_slice(&4u16.to_be_bytes());
            buf.extend_from_slice(b"ping");
            buf
        })
        .unwrap();

        assert_eq!(frame.meta.session_id, 42);
        assert_eq!(frame.meta.status, SESSION_STATUS_NEW);
        assert_eq!(frame.meta.target, Some(target));
        assert_eq!(frame.payload.as_deref(), Some(b"ping" as &[u8]));
    }

    #[test]
    fn xudp_new_meta_roundtrip() {
        let target = Endpoint::Ip(SocketAddr::new(Ipv4Addr::new(8, 8, 8, 8).into(), 53));
        let global_id = [0xAB; 8];
        let meta = MuxMeta {
            session_id: XUDP_SESSION_ID,
            status: SESSION_STATUS_NEW,
            option: OPTION_DATA,
            network: Some(NETWORK_UDP),
            target: Some(target.clone()),
            global_id: Some(global_id),
        };

        let encoded = encode_meta(&meta);
        let parsed = parse_meta_body(&encoded[2..]).unwrap();
        assert_eq!(parsed.session_id, 0);
        assert_eq!(parsed.target, Some(target));
        assert_eq!(parsed.global_id, Some(global_id));
    }

    #[test]
    fn xudp_keep_meta_roundtrip() {
        let target = Endpoint::Ip(SocketAddr::new(Ipv4Addr::new(1, 1, 1, 1).into(), 53));
        let meta = MuxMeta {
            session_id: XUDP_SESSION_ID,
            status: SESSION_STATUS_KEEP,
            option: OPTION_DATA,
            network: Some(NETWORK_UDP),
            target: Some(target.clone()),
            global_id: None,
        };

        let encoded = encode_meta(&meta);
        let parsed = parse_meta_body(&encoded[2..]).unwrap();
        assert_eq!(parsed.session_id, 0);
        assert_eq!(parsed.status, SESSION_STATUS_KEEP);
        assert_eq!(parsed.target, Some(target));
    }

    #[test]
    fn end_frame_has_no_payload() {
        let frame = encode_end_frame(7, false);
        let (parsed, len) = parse_frame(&frame).unwrap();
        assert_eq!(len, frame.len());
        assert_eq!(parsed.meta.status, SESSION_STATUS_END);
        assert!(parsed.payload.is_none());
    }
}
