//! XHTTP session registry для stream-up (GET downlink + POST uplink).

use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_util::io::StreamReader;

const UPLOAD_QUEUE_SIZE: usize = 256;

/// Состояние одной XHTTP-сессии (stream-up).
pub struct XhttpSession {
    upload_tx: mpsc::Sender<Bytes>,
    upload_rx: Mutex<Option<mpsc::Receiver<Bytes>>>,
}

impl XhttpSession {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel(UPLOAD_QUEUE_SIZE);
        Self {
            upload_tx: tx,
            upload_rx: Mutex::new(Some(rx)),
        }
    }

    /// Забрать uplink-очередь (только один GET на сессию).
    pub fn take_upload_rx(&self) -> Option<mpsc::Receiver<Bytes>> {
        self.upload_rx.lock().unwrap().take()
    }

    /// Подключить POST body как uplink (копирует в очередь до EOF).
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
