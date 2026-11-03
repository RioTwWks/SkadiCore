//! XHTTP session registry для stream-up и packet-up.

use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_util::io::StreamReader;

const UPLOAD_QUEUE_SIZE: usize = 256;
/// Лимит размера одного packet-up POST (как Xray scMaxEachPostBytes по умолчанию).
pub const DEFAULT_MAX_POST_BYTES: usize = 1_000_000;

// ─── Анти-DoS лимиты (сессии создаются до VLESS-аутентификации) ───

/// Максимум одновременно отслеживаемых сессий в реестре.
const MAX_SESSIONS: usize = 1024;
/// Сессия без активности дольше этого срока вытесняется при уборке.
const SESSION_IDLE_TTL_SECS: u64 = 120;
/// Пакеты с seq дальше этого окна от next_seq отбрасываются.
const MAX_SEQ_WINDOW: u64 = 1024;
/// Максимум пакетов в буфере переупорядочивания одной сессии.
const MAX_PENDING_PACKETS: usize = 64;
/// Максимум байт в буфере переупорядочивания одной сессии.
const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

struct PacketState {
    next_seq: u64,
    pending: BTreeMap<u64, Bytes>,
    pending_bytes: usize,
}

impl PacketState {
    fn new() -> Self {
        Self {
            next_seq: 0,
            pending: BTreeMap::new(),
            pending_bytes: 0,
        }
    }

    fn push(&mut self, seq: u64, payload: Bytes, upload_tx: &mpsc::Sender<Bytes>) {
        if seq < self.next_seq {
            return;
        }
        // Анти-DoS: далёкий seq никогда не будет запрошен заново и лишь
        // раздувает буфер переупорядочивания.
        if seq >= self.next_seq.saturating_add(MAX_SEQ_WINDOW) {
            return;
        }
        if self.pending.contains_key(&seq) {
            return;
        }
        if self.pending.len() >= MAX_PENDING_PACKETS
            || self.pending_bytes + payload.len() > MAX_PENDING_BYTES
        {
            return;
        }
        self.pending_bytes += payload.len();
        self.pending.insert(seq, payload);
        while let Some(chunk) = self.pending.remove(&self.next_seq) {
            self.pending_bytes = self.pending_bytes.saturating_sub(chunk.len());
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
    /// Последняя активность (unix seconds) для TTL-уборки реестра.
    last_seen: AtomicU64,
}

impl XhttpSession {
    fn new(now: u64) -> Self {
        let (tx, rx) = mpsc::channel(UPLOAD_QUEUE_SIZE);
        Self {
            upload_tx: tx,
            upload_rx: Mutex::new(Some(rx)),
            packets: Mutex::new(PacketState::new()),
            last_seen: AtomicU64::new(now),
        }
    }

    fn touch(&self, now: u64) {
        self.last_seen.store(now, Ordering::Relaxed);
    }

    fn last_seen(&self) -> u64 {
        self.last_seen.load(Ordering::Relaxed)
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
        self.touch(now_secs());
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
        let now = now_secs();
        let mut guard = self.sessions.lock().unwrap();
        if let Some(session) = guard.get(session_id) {
            session.touch(now);
            return Arc::clone(session);
        }
        // Ленивая TTL-уборка простаивающих сессий.
        guard.retain(|_, s| now.saturating_sub(s.last_seen()) < SESSION_IDLE_TTL_SECS);
        // Потолок реестра: вытеснить самую неактивную сессию.
        if guard.len() >= MAX_SESSIONS {
            if let Some(oldest) = guard
                .iter()
                .min_by_key(|(_, s)| s.last_seen())
                .map(|(k, _)| k.clone())
            {
                guard.remove(&oldest);
            }
        }
        let session = Arc::new(XhttpSession::new(now));
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
        let session = XhttpSession::new(now_secs());
        let mut rx = session.take_upload_rx().unwrap();
        session.push_packet(0, Bytes::from_static(b"ab"));
        session.push_packet(1, Bytes::from_static(b"cd"));
        assert_eq!(rx.try_recv().unwrap(), Bytes::from_static(b"ab"));
        assert_eq!(rx.try_recv().unwrap(), Bytes::from_static(b"cd"));
    }

    #[test]
    fn packet_reassembly_out_of_order() {
        let session = XhttpSession::new(now_secs());
        let mut rx = session.take_upload_rx().unwrap();
        session.push_packet(1, Bytes::from_static(b"B"));
        assert!(rx.try_recv().is_err());
        session.push_packet(0, Bytes::from_static(b"A"));
        assert_eq!(rx.try_recv().unwrap(), Bytes::from_static(b"A"));
        assert_eq!(rx.try_recv().unwrap(), Bytes::from_static(b"B"));
    }

    #[test]
    fn packet_beyond_seq_window_dropped() {
        let session = XhttpSession::new(now_secs());
        let mut rx = session.take_upload_rx().unwrap();
        session.push_packet(u64::MAX, Bytes::from_static(b"x"));
        session.push_packet(MAX_SEQ_WINDOW + 10, Bytes::from_static(b"y"));
        assert!(rx.try_recv().is_err());
        let state = session.packets.lock().unwrap();
        assert!(state.pending.is_empty());
    }

    #[test]
    fn pending_buffer_is_bounded() {
        let session = XhttpSession::new(now_secs());
        // Заполнить буфер «дырой»: seq=0 не приходит, остальное копится.
        for seq in 1..=(MAX_PENDING_PACKETS as u64 + 10) {
            session.push_packet(seq, Bytes::from_static(b"chunk"));
        }
        let state = session.packets.lock().unwrap();
        assert!(state.pending.len() <= MAX_PENDING_PACKETS);
    }

    #[test]
    fn session_registry_is_bounded() {
        let manager = XhttpSessionManager::new();
        for i in 0..(MAX_SESSIONS + 100) {
            manager.get_or_create(&format!("session-{}", i));
        }
        let guard = manager.sessions.lock().unwrap();
        assert!(guard.len() <= MAX_SESSIONS);
    }
}
