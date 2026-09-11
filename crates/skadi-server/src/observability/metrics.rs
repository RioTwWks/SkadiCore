//! Prometheus-метрики SkadiCore (без PII).

use metrics::{counter, describe_counter, describe_gauge, gauge};
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_exporter_prometheus::PrometheusHandle;

/// Установить глобальный Prometheus recorder.
pub fn install_recorder() -> anyhow::Result<PrometheusHandle> {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| anyhow::anyhow!("failed to install prometheus recorder: {}", e))?;

    describe_gauge!(
        "skadicore_active_connections",
        "Number of active proxy relay sessions"
    );
    describe_counter!(
        "skadicore_connections_total",
        "Total proxy connections by protocol and event"
    );
    describe_counter!(
        "skadicore_transfer_bytes_total",
        "Total bytes relayed through the proxy"
    );
    describe_counter!(
        "skadicore_connections_rejected_total",
        "Inbound connections rejected because max_connections limit was reached"
    );

    Ok(handle)
}

pub fn connection_opened(protocol: &str) {
    gauge!("skadicore_active_connections").increment(1.0);
    counter!(
        "skadicore_connections_total",
        "protocol" => protocol.to_string(),
        "event" => "opened"
    )
    .increment(1);
}

pub fn connection_closed(protocol: &str, up: u64, down: u64) {
    gauge!("skadicore_active_connections").decrement(1.0);
    counter!(
        "skadicore_connections_total",
        "protocol" => protocol.to_string(),
        "event" => "closed"
    )
    .increment(1);
    if up > 0 {
        counter!(
            "skadicore_transfer_bytes_total",
            "direction" => "up"
        )
        .increment(up);
    }
    if down > 0 {
        counter!(
            "skadicore_transfer_bytes_total",
            "direction" => "down"
        )
        .increment(down);
    }
}

pub fn connection_failed(protocol: &str) {
    gauge!("skadicore_active_connections").decrement(1.0);
    counter!(
        "skadicore_connections_total",
        "protocol" => protocol.to_string(),
        "event" => "failed"
    )
    .increment(1);
}

pub fn connection_rejected() {
    counter!("skadicore_connections_rejected_total").increment(1);
}
