//! Общие хелперы для интеграционных тестов.

pub mod awg_fixtures;
pub mod xray;

use skadi_server::config::OutboundConfig;

/// Outbound для тестов с локальными echo/upstream на loopback.
pub fn test_outbound() -> OutboundConfig {
    OutboundConfig {
        allow_private: true,
        ..Default::default()
    }
}
