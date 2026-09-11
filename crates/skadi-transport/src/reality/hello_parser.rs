//! Минимальный парсер TLS ClientHello для REALITY pre-check.

use anyhow::{anyhow, Result};

/// Данные ClientHello, нужные для REALITY-аутентификации.
#[derive(Debug, Clone)]
pub struct ClientHelloInfo {
    pub session_id: Vec<u8>,
    pub client_random: [u8; 32],
    pub public_key: Option<Vec<u8>>,
    pub server_name: Option<String>,
}

/// Разобрать ClientHello из сырого TLS record (или handshake payload).
pub fn parse_client_hello(buf: &[u8]) -> Result<Option<ClientHelloInfo>> {
    if buf.len() < 5 || buf[0] != 0x16 {
        return Ok(None);
    }

    let record_len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
    if buf.len() < 5 + record_len {
        return Ok(None);
    }

    let mut offset = 5usize;

    if offset + 4 > buf.len() {
        return Err(anyhow!("short buffer"));
    }
    if buf[offset] != 0x01 {
        return Ok(None);
    }
    offset += 4;

    if offset + 2 > buf.len() {
        return Err(anyhow!("short buffer for version"));
    }
    offset += 2;

    if offset + 32 > buf.len() {
        return Err(anyhow!("short buffer for random"));
    }
    let mut client_random = [0u8; 32];
    client_random.copy_from_slice(&buf[offset..offset + 32]);
    offset += 32;

    if offset >= buf.len() {
        return Err(anyhow!("short buffer for session id len"));
    }
    let session_id_len = buf[offset] as usize;
    offset += 1;
    if offset + session_id_len > buf.len() {
        return Err(anyhow!("short buffer for session id"));
    }
    let session_id = buf[offset..offset + session_id_len].to_vec();
    offset += session_id_len;

    if offset + 2 > buf.len() {
        return Err(anyhow!("short buffer for cipher suites len"));
    }
    let cipher_suites_len = u16::from_be_bytes([buf[offset], buf[offset + 1]]) as usize;
    offset += 2;
    if offset + cipher_suites_len > buf.len() {
        return Err(anyhow!("short buffer for cipher suites"));
    }
    offset += cipher_suites_len;

    if offset >= buf.len() {
        return Err(anyhow!("short buffer for compression methods len"));
    }
    let compression_methods_len = buf[offset] as usize;
    offset += 1;
    if offset + compression_methods_len > buf.len() {
        return Err(anyhow!("short buffer for compression methods"));
    }
    offset += compression_methods_len;

    if offset + 2 > buf.len() {
        return Ok(Some(ClientHelloInfo {
            session_id,
            client_random,
            public_key: None,
            server_name: None,
        }));
    }

    let extensions_len = u16::from_be_bytes([buf[offset], buf[offset + 1]]) as usize;
    offset += 2;
    if offset + extensions_len > buf.len() {
        return Err(anyhow!("short buffer for extensions"));
    }

    let extensions = &buf[offset..offset + extensions_len];
    let (public_key, server_name) = parse_extensions(extensions);

    Ok(Some(ClientHelloInfo {
        session_id,
        client_random,
        public_key,
        server_name,
    }))
}

fn parse_extensions(extensions: &[u8]) -> (Option<Vec<u8>>, Option<String>) {
    let mut public_key = None;
    let mut server_name = None;
    let mut offset = 0usize;

    while offset + 4 <= extensions.len() {
        let ext_type = u16::from_be_bytes([extensions[offset], extensions[offset + 1]]);
        let ext_len = u16::from_be_bytes([extensions[offset + 2], extensions[offset + 3]]) as usize;
        offset += 4;
        if offset + ext_len > extensions.len() {
            break;
        }
        let ext_data = &extensions[offset..offset + ext_len];
        offset += ext_len;

        if ext_type == 0x0000 {
            server_name = parse_sni(ext_data);
        } else if ext_type == 0x0033 {
            public_key = parse_key_share(ext_data);
        }

        if public_key.is_some() && server_name.is_some() {
            break;
        }
    }

    (public_key, server_name)
}

fn parse_sni(ext_data: &[u8]) -> Option<String> {
    if ext_data.len() < 2 {
        return None;
    }
    let list_len = u16::from_be_bytes([ext_data[0], ext_data[1]]) as usize;
    if ext_data.len() < 2 + list_len {
        return None;
    }

    let mut offset = 2usize;
    let end = 2 + list_len;
    while offset + 3 <= end {
        let name_type = ext_data[offset];
        let name_len = u16::from_be_bytes([ext_data[offset + 1], ext_data[offset + 2]]) as usize;
        offset += 3;
        if offset + name_len > end {
            break;
        }
        if name_type == 0x00 {
            return String::from_utf8(ext_data[offset..offset + name_len].to_vec()).ok();
        }
        offset += name_len;
    }
    None
}

fn parse_key_share(ext_data: &[u8]) -> Option<Vec<u8>> {
    if ext_data.len() < 2 {
        return None;
    }
    let shares_len = u16::from_be_bytes([ext_data[0], ext_data[1]]) as usize;
    if ext_data.len() < 2 + shares_len {
        return None;
    }

    let mut offset = 2usize;
    let end = 2 + shares_len;
    while offset + 4 <= end {
        let group = u16::from_be_bytes([ext_data[offset], ext_data[offset + 1]]);
        let key_len = u16::from_be_bytes([ext_data[offset + 2], ext_data[offset + 3]]) as usize;
        offset += 4;
        if offset + key_len > end {
            break;
        }
        if group == 0x001d && key_len == 32 {
            return Some(ext_data[offset..offset + 32].to_vec());
        }
        offset += key_len;
    }
    None
}
