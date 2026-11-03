//! Per-IP rate limiting на неудачные аутентификации.

use crate::config::AuthRateLimitConfig;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::debug;

/// Потолок записей в карте состояний: защита от раздувания памяти
/// при потоке неудачных аутентификаций с уникальных IP (ботнет/спуфинг).
const MAX_TRACKED_IPS: usize = 65_536;

struct IpState {
    failures: u32,
    window_start: Instant,
    blocked_until: Option<Instant>,
    ban_level: u32,
    /// Последнее обращение — для TTL-очистки карты.
    last_seen: Instant,
}

struct LimiterInner {
    max_failures: u32,
    window: Duration,
    ban_base: Duration,
    ban_max: Duration,
    states: Mutex<HashMap<IpAddr, IpState>>,
}

/// Ограничение неудачных аутентификаций с одного IP.
#[derive(Clone)]
pub struct AuthRateLimiter {
    inner: Arc<LimiterInner>,
}

impl AuthRateLimiter {
    pub fn from_config(config: &AuthRateLimitConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        let max_failures = config.max_failures.filter(|n| *n > 0)?;
        Some(Self {
            inner: Arc::new(LimiterInner {
                max_failures,
                window: Duration::from_secs(config.window_secs),
                ban_base: Duration::from_secs(config.ban_base_secs),
                ban_max: Duration::from_secs(config.ban_max_secs),
                states: Mutex::new(HashMap::new()),
            }),
        })
    }

    /// Горизонт устаревания записи: переживает и окно подсчёта, и самый долгий бан.
    fn retention(&self) -> Duration {
        self.inner.window.max(self.inner.ban_max) + Duration::from_secs(60)
    }

    pub fn is_blocked(&self, ip: IpAddr) -> bool {
        let mut guard = self.inner.states.lock().expect("auth rate limiter lock");
        let now = Instant::now();
        self.cleanup_expired(&mut guard, now);

        match guard.get_mut(&ip) {
            Some(state) => {
                state.last_seen = now;
                if let Some(until) = state.blocked_until {
                    if now < until {
                        return true;
                    }
                    state.blocked_until = None;
                }
                false
            }
            None => false,
        }
    }

    pub fn record_failure(&self, ip: IpAddr) {
        let mut guard = self.inner.states.lock().expect("auth rate limiter lock");
        let now = Instant::now();
        self.cleanup_expired(&mut guard, now);

        // Потолок карты: вытеснить самую старую незабаненную запись.
        if !guard.contains_key(&ip) && guard.len() >= MAX_TRACKED_IPS {
            let oldest = guard
                .iter()
                .filter(|(_, s)| s.blocked_until.is_none_or(|until| now >= until))
                .min_by_key(|(_, s)| s.last_seen)
                .map(|(k, _)| *k);
            if let Some(oldest) = oldest {
                guard.remove(&oldest);
            } else {
                // Все записи забанены — удалить бан с ближайшим истечением.
                let soonest = guard
                    .iter()
                    .min_by_key(|(_, s)| s.blocked_until)
                    .map(|(k, _)| *k);
                if let Some(soonest) = soonest {
                    guard.remove(&soonest);
                }
            }
        }

        let state = guard.entry(ip).or_insert_with(|| IpState {
            failures: 0,
            window_start: now,
            blocked_until: None,
            ban_level: 0,
            last_seen: now,
        });
        state.last_seen = now;

        if now.duration_since(state.window_start) >= self.inner.window {
            state.window_start = now;
            state.failures = 0;
        }

        state.failures += 1;
        if state.failures < self.inner.max_failures {
            return;
        }

        state.failures = 0;
        state.window_start = now;
        state.ban_level = state.ban_level.saturating_add(1);
        let multiplier = 1u32 << state.ban_level.min(10);
        let ban = self
            .inner
            .ban_base
            .saturating_mul(multiplier)
            .min(self.inner.ban_max);
        state.blocked_until = Some(now + ban);

        debug!(
            ip = %ip,
            ban_secs = ban.as_secs(),
            ban_level = state.ban_level,
            "auth rate limit ban applied"
        );
    }

    /// Удалить записи, к которым не обращались дольше горизонта устаревания.
    fn cleanup_expired(&self, guard: &mut HashMap<IpAddr, IpState>, now: Instant) {
        let retention = self.retention();
        guard.retain(|_, state| now.duration_since(state.last_seen) < retention);
    }
}

pub fn is_auth_failure(err: &anyhow::Error) -> bool {
    let msg = err.to_string();
    msg.contains("authentication failed") || msg.contains("unsupported flow")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(max_failures: u32) -> AuthRateLimitConfig {
        AuthRateLimitConfig {
            enabled: true,
            max_failures: Some(max_failures),
            window_secs: 600,
            ban_base_secs: 60,
            ban_max_secs: 3600,
        }
    }

    #[test]
    fn blocks_after_max_failures() {
        let limiter = AuthRateLimiter::from_config(&test_config(2)).unwrap();
        let ip = "203.0.113.1".parse().unwrap();
        assert!(!limiter.is_blocked(ip));
        limiter.record_failure(ip);
        assert!(!limiter.is_blocked(ip));
        limiter.record_failure(ip);
        assert!(limiter.is_blocked(ip));
    }

    #[test]
    fn disabled_when_not_enabled() {
        let config = AuthRateLimitConfig {
            enabled: false,
            max_failures: Some(5),
            window_secs: 600,
            ban_base_secs: 60,
            ban_max_secs: 3600,
        };
        assert!(AuthRateLimiter::from_config(&config).is_none());
    }

    #[test]
    fn tracked_ips_map_is_bounded() {
        let limiter = AuthRateLimiter::from_config(&test_config(5)).unwrap();
        for i in 0..(MAX_TRACKED_IPS + 100) {
            let ip = IpAddr::from([10, (i >> 16) as u8, (i >> 8) as u8, i as u8]);
            limiter.record_failure(ip);
        }
        let guard = limiter.inner.states.lock().unwrap();
        assert!(guard.len() <= MAX_TRACKED_IPS);
    }
}
