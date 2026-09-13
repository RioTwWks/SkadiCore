//! XUDP GlobalID manager (Xray `common/mux` XUDPManager).

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::task::AbortHandle;
use tracing::debug;

const XUDP_EXPIRE_TTL: Duration = Duration::from_secs(60);
const XUDP_CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum XudpStatus {
    Active,
    Initializing,
    Expiring,
}

struct XudpEntry {
    socket: Arc<UdpSocket>,
    status: XudpStatus,
    expire_at: Option<Instant>,
    reader_abort: Option<AbortHandle>,
}

/// Глобальный реестр XUDP cone-сессий по GlobalID.
pub struct XudpManager {
    entries: Mutex<HashMap<[u8; 8], XudpEntry>>,
}

impl XudpManager {
    fn global() -> &'static Arc<Self> {
        static INSTANCE: OnceLock<Arc<XudpManager>> = OnceLock::new();
        INSTANCE.get_or_init(|| {
            let manager = Arc::new(XudpManager {
                entries: Mutex::new(HashMap::new()),
            });
            let cleanup_target = Arc::clone(&manager);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(XUDP_CLEANUP_INTERVAL);
                interval.tick().await;
                loop {
                    interval.tick().await;
                    cleanup_target.cleanup_expired().await;
                }
            });
            manager
        })
    }

    /// Получить или создать UDP-сокет для GlobalID. `true` = hit (reuse).
    pub async fn acquire(global_id: [u8; 8]) -> io::Result<(Arc<UdpSocket>, bool)> {
        Self::global().acquire_inner(global_id).await
    }

    /// Отсоединить mux от cone: оставить сокет в реестре на TTL.
    pub async fn detach(global_id: [u8; 8]) {
        Self::global().detach_inner(global_id).await;
    }

    /// Зарегистрировать reader task для последующего abort при hit/detach.
    pub async fn register_reader(global_id: [u8; 8], abort: AbortHandle) {
        Self::global().register_reader_inner(global_id, abort).await;
    }

    async fn acquire_inner(&self, global_id: [u8; 8]) -> io::Result<(Arc<UdpSocket>, bool)> {
        let mut guard = self.entries.lock().await;
        if let Some(entry) = guard.get_mut(&global_id) {
            if entry.status == XudpStatus::Initializing {
                return Ok((Arc::clone(&entry.socket), true));
            }
            entry.status = XudpStatus::Initializing;
            if let Some(abort) = entry.reader_abort.take() {
                abort.abort();
            }
            entry.expire_at = None;
            entry.status = XudpStatus::Active;
            debug!(global_id = ?global_id, "xudp hit");
            return Ok((Arc::clone(&entry.socket), true));
        }

        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        guard.insert(
            global_id,
            XudpEntry {
                socket: Arc::new(socket),
                status: XudpStatus::Active,
                expire_at: None,
                reader_abort: None,
            },
        );
        let socket = Arc::clone(&guard.get(&global_id).unwrap().socket);
        debug!(global_id = ?global_id, "xudp new");
        Ok((socket, false))
    }

    async fn detach_inner(&self, global_id: [u8; 8]) {
        let mut guard = self.entries.lock().await;
        if let Some(entry) = guard.get_mut(&global_id) {
            if entry.status == XudpStatus::Active {
                if let Some(abort) = entry.reader_abort.take() {
                    abort.abort();
                }
                entry.status = XudpStatus::Expiring;
                entry.expire_at = Some(Instant::now() + XUDP_EXPIRE_TTL);
                debug!(global_id = ?global_id, "xudp expiring");
            }
        }
    }

    async fn register_reader_inner(&self, global_id: [u8; 8], abort: AbortHandle) {
        let mut guard = self.entries.lock().await;
        if let Some(entry) = guard.get_mut(&global_id) {
            entry.reader_abort = Some(abort);
        }
    }

    async fn cleanup_expired(&self) {
        let now = Instant::now();
        let mut guard = self.entries.lock().await;
        let expired: Vec<[u8; 8]> = guard
            .iter()
            .filter(|(_, e)| {
                e.status == XudpStatus::Expiring && e.expire_at.is_some_and(|t| now >= t)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if let Some(entry) = guard.remove(&id) {
                if let Some(abort) = entry.reader_abort {
                    abort.abort();
                }
                debug!(global_id = ?id, "xudp deleted");
            }
        }
    }

    #[cfg(test)]
    fn new_for_test() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn manager_hit_reuses_socket() {
        let mgr = XudpManager::new_for_test();
        let (s1, hit1) = mgr.acquire_inner([0xAB; 8]).await.unwrap();
        assert!(!hit1);

        mgr.detach_inner([0xAB; 8]).await;

        let (s2, hit2) = mgr.acquire_inner([0xAB; 8]).await.unwrap();
        assert!(hit2);
        assert!(Arc::ptr_eq(&s1, &s2));
    }

    #[tokio::test]
    async fn manager_cleanup_removes_expired() {
        let mgr = XudpManager::new_for_test();
        let id = [0xCD; 8];
        mgr.acquire_inner(id).await.unwrap();
        {
            let mut guard = mgr.entries.lock().await;
            let entry = guard.get_mut(&id).unwrap();
            entry.status = XudpStatus::Expiring;
            entry.expire_at = Some(Instant::now() - Duration::from_secs(1));
        }
        mgr.cleanup_expired().await;
        assert!(mgr.entries.lock().await.get(&id).is_none());
    }
}
