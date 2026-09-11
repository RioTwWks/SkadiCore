use crate::user::UserId;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(1);

/// Уникальный идентификатор сессии. Не содержит PII, только счётчик.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(pub u64);

impl SessionId {
    pub fn next() -> Self {
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

/// Метаданные активной сессии. Используется для логов и метрик.
#[derive(Debug)]
pub struct Session {
    pub id: SessionId,
    pub user: Option<UserId>,
    pub peer: std::net::SocketAddr,
}

impl Session {
    pub fn new(peer: std::net::SocketAddr) -> Self {
        Self {
            id: SessionId::next(),
            user: None,
            peer,
        }
    }

    pub fn with_user(mut self, user: UserId) -> Self {
        self.user = Some(user);
        self
    }
}
