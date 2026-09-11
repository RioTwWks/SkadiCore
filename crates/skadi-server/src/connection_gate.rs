//! Лимит одновременных inbound-соединений (backpressure).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Счётчик активных сессий с опциональным потолком. `None` — без лимита.
#[derive(Clone)]
pub struct ConnectionGate {
    max: Option<u32>,
    active: Arc<AtomicU32>,
}

impl ConnectionGate {
    pub fn new(max_connections: Option<u32>) -> Self {
        Self {
            max: max_connections,
            active: Arc::new(AtomicU32::new(0)),
        }
    }

    #[cfg(test)]
    pub fn active(&self) -> u32 {
        self.active.load(Ordering::Acquire)
    }

    /// Захватить слот без ожидания. `None` — лимит исчерпан.
    pub fn try_acquire(&self) -> Option<ConnectionPermit> {
        match self.max {
            None => Some(ConnectionPermit { active: None }),
            Some(max) => {
                let mut current = self.active.load(Ordering::Acquire);
                while current < max {
                    match self.active.compare_exchange_weak(
                        current,
                        current + 1,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => {
                            return Some(ConnectionPermit {
                                active: Some(Arc::clone(&self.active)),
                            });
                        }
                        Err(actual) => current = actual,
                    }
                }
                None
            }
        }
    }
}

/// Держит слот до конца сессии.
pub struct ConnectionPermit {
    active: Option<Arc<AtomicU32>>,
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        if let Some(active) = &self.active {
            active.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_always_acquires() {
        let gate = ConnectionGate::new(None);
        assert!(gate.try_acquire().is_some());
        assert!(gate.try_acquire().is_some());
        assert_eq!(gate.active(), 0);
    }

    #[test]
    fn limited_rejects_when_full() {
        let gate = ConnectionGate::new(Some(1));
        let permit = gate.try_acquire().expect("first slot");
        assert_eq!(gate.active(), 1);
        assert!(gate.try_acquire().is_none());
        drop(permit);
        assert_eq!(gate.active(), 0);
        assert!(gate.try_acquire().is_some());
    }
}
