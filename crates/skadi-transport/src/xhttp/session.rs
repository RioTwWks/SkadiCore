//! XHTTP session registry для stream-up и packet-up.

use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_util::io::StreamReader;

const UPLOAD_QUEUE_SIZE: usize = 256;
/// Лимит размера одного packet-up POST (как Xray scMaxEachPostBytes по умолчанию).
pub const DEFAULT_MAX_POST_BYTES: usize = 1_000_000;

struct PacketState {
    next_seq: u64,
    pending: BTreeMap<u64, Bytes>,
}

impl PacketState {
    fn new() -> Self {
        Self {
            next_seq: 0,
            pending: BTreeMap::new(),
        }
    }

    fn push(&mut self, seq: u64, payload: Bytes, upload_tx: &mpsc::Sender<Bytes>) {
        if seq < self.next_seq {
            return;
        }
        self.pending.insert(seq, payload);
        while let Some(chunk) = self.pending.remove(&self.next_seq) {
            self.next_seq += 1;
            let _ = upload_tx.try_send(chunk);
        }
    }
}

/// Состояние одной XHTTP-сессии.
pub struct XhttpSession {
    upload_tx: mpsc::Sender<Bytes>,
    upload_rx: Mutex<Option<mpsc::Receiver<Bytes>>>,
    packets: Mutex<PacketState>,
}

impl XhttpSession {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel(UPLOAD_QUEUE_SIZE);
        Self {
            upload_tx: tx,
            upload_rx: Mutex::new(Some(rx)),
            packets: Mutex::new(PacketState::new()),
        }
    }

    /// Забрать uplink-очередь (только один GET на сессию).
    pub fn take_upload_rx(&self) -> Option<mpsc::Receiver<Bytes>> {
        self.upload_rx.lock().unwrap().take()
    }

    /// Stream-up: подключить POST body как непрерывный uplink.
    pub async fn attach_post_body(&self, body: Incoming) {
        let mut stream = StreamReader::new(body.into_data_stream().map_err(std::io::Error::other));
        let mut chunk = [0u8; 8192];
        while let Ok(n) = stream.read(&mut chunk).await {
            if n == 0 {
                break;
            }
            let _ = self
                .upload_tx
                .send(Bytes::copy_from_slice(&chunk[..n]))
                .await;
        }
    }

    /// Packet-up: принять sequenced payload.
    pub fn push_packet(&self, seq: u64, payload: Bytes) {
        let mut state = self.packets.lock().unwrap();
        state.push(seq, payload, &self.upload_tx);
    }
}

/// Глобальный реестр XHTTP-сессий на сервере.
#[derive(Default)]
pub struct XhttpSessionManager {
    sessions: Mutex<HashMap<String, Arc<XhttpSession>>>,
}

impl XhttpSessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_create(&self, session_id: &str) -> Arc<XhttpSession> {
        let mut guard = self.sessions.lock().unwrap();
        if let Some(session) = guard.get(session_id) {
            return Arc::clone(session);
        }
        let session = Arc::new(XhttpSession::new());
        guard.insert(session_id.to_string(), Arc::clone(&session));
        session
    }

    pub fn remove(&self, session_id: &str) {
        self.sessions.lock().unwrap().remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_reassembly_in_order() {
        let session = XhttpSession::new();
        let mut rx = session.take_upload_rx().unwrap();
        session.push_packet(0, Bytes::from_static(b"ab"));
        session.push_packet(1, Bytes::from_static(b"cd"));
        assert_eq!(rx.try_recv().unwrap(), Bytes::from_static(b"ab"));
        assert_eq!(rx.try_recv().unwrap(), Bytes::from_static(b"cd"));
    }

    #[test]
    fn packet_reassembly_out_of_order() {
        let session = XhttpSession::new();
        let mut rx = session.take_upload_rx().unwrap();
        session.push_packet(1, Bytes::from_static(b"B"));
        assert!(rx.try_recv().is_err());
        session.push_packet(0, Bytes::from_static(b"A"));
        assert_eq!(rx.try_recv().unwrap(), Bytes::from_static(b"A"));
        assert_eq!(rx.try_recv().unwrap(), Bytes::from_static(b"B"));
    }
}
