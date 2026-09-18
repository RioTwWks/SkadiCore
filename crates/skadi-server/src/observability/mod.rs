//! Наблюдаемость: Prometheus `/metrics` и `/healthz`.

mod http;
mod metrics;
pub mod tracing_init;

pub use http::run_metrics_server;
pub use metrics::{
    auth_blocked, auth_failure, connection_closed, connection_failed, connection_opened,
    connection_rejected, install_recorder,
};
