//! Fixed-window rate limiter for gRPC API.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tonic::Status;

struct WindowState {
    start: Instant,
    count: u32,
}

struct LimiterInner {
    limit: u32,
    window: Duration,
    state: Mutex<WindowState>,
}

/// Ограничение числа RPC в секунду (глобально на процесс).
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
                state: Mutex::new(WindowState {
                    start: Instant::now(),
                    count: 0,
                }),
            }),
        })
    }

    pub fn check(&self) -> Result<(), Status> {
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| Status::internal("rate limiter unavailable"))?;
        let now = Instant::now();
        if now.duration_since(guard.start) >= self.inner.window {
            guard.start = now;
            guard.count = 0;
        }
        if guard.count >= self.inner.limit {
            return Err(Status::resource_exhausted("rate limit exceeded"));
        }
        guard.count += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_limit_per_window() {
        let limiter = ApiRateLimiter::new(Some(2)).unwrap();
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_err());
    }

    #[test]
    fn disabled_when_zero_or_none() {
        assert!(ApiRateLimiter::new(None).is_none());
        assert!(ApiRateLimiter::new(Some(0)).is_none());
    }
}
