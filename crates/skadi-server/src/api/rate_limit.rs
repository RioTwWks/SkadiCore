//! Per-IP fixed-window rate limiter for gRPC API.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tonic::Status;

/// Потолок записей о пирах: защита карты от раздувания при флуде с уникальных IP.
const MAX_TRACKED_PEERS: usize = 4096;

struct WindowState {
    start: Instant,
    count: u32,
}

struct LimiterInner {
    limit: u32,
    window: Duration,
    states: Mutex<HashMap<IpAddr, WindowState>>,
}

/// Ограничение числа RPC в секунду с одного IP.
///
/// Лимит per-IP, а не глобальный: флуд с чужого адреса не должен
/// блокировать легитимного администратора (DoS management API).
#[derive(Clone)]
pub struct ApiRateLimiter {
    inner: Arc<LimiterInner>,
}

impl ApiRateLimiter {
    pub fn new(limit_per_sec: Option<u32>) -> Option<Self> {
        let limit = limit_per_sec.filter(|n| *n > 0)?;
        Some(Self {
            inner: Arc::new(LimiterInner {
                limit,
                window: Duration::from_secs(1),
                states: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn check(&self, peer: Option<IpAddr>) -> Result<(), Status> {
        // Пиры без адреса (UDS и т.п.) делят общую корзину.
        let key = peer.unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        let mut guard = self
            .inner
            .states
            .lock()
            .map_err(|_| Status::internal("rate limiter unavailable"))?;
        let now = Instant::now();

        if !guard.contains_key(&key) && guard.len() >= MAX_TRACKED_PEERS {
            // Сначала выкинуть протухшие окна, затем — самое старое.
            guard.retain(|_, s| now.duration_since(s.start) < self.inner.window);
            if guard.len() >= MAX_TRACKED_PEERS {
                if let Some(oldest) = guard
                    .iter()
                    .min_by_key(|(_, s)| s.start)
                    .map(|(k, _)| *k)
                {
                    guard.remove(&oldest);
                }
            }
        }

        let state = guard.entry(key).or_insert_with(|| WindowState {
            start: now,
            count: 0,
        });
        if now.duration_since(state.start) >= self.inner.window {
            state.start = now;
            state.count = 0;
        }
        if state.count >= self.inner.limit {
            return Err(Status::resource_exhausted("rate limit exceeded"));
        }
        state.count += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_limit_per_window() {
        let limiter = ApiRateLimiter::new(Some(2)).unwrap();
        let peer: IpAddr = "203.0.113.1".parse().unwrap();
        assert!(limiter.check(Some(peer)).is_ok());
        assert!(limiter.check(Some(peer)).is_ok());
        assert!(limiter.check(Some(peer)).is_err());
    }

    #[test]
    fn limit_is_per_peer_not_global() {
        let limiter = ApiRateLimiter::new(Some(1)).unwrap();
        let a: IpAddr = "203.0.113.1".parse().unwrap();
        let b: IpAddr = "203.0.113.2".parse().unwrap();
        assert!(limiter.check(Some(a)).is_ok());
        assert!(limiter.check(Some(a)).is_err());
        // Другой пир не заблокирован флудом с первого адреса.
        assert!(limiter.check(Some(b)).is_ok());
    }

    #[test]
    fn disabled_when_zero_or_none() {
        assert!(ApiRateLimiter::new(None).is_none());
        assert!(ApiRateLimiter::new(Some(0)).is_none());
    }

    #[test]
    fn peer_map_is_bounded() {
        let limiter = ApiRateLimiter::new(Some(1000)).unwrap();
        for i in 0..(MAX_TRACKED_PEERS + 100) {
            let ip = IpAddr::from([10, (i >> 16) as u8, (i >> 8) as u8, i as u8]);
            let _ = limiter.check(Some(ip));
        }
        let guard = limiter.inner.states.lock().unwrap();
        assert!(guard.len() <= MAX_TRACKED_PEERS);
    }
}
