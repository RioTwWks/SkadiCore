//! Наблюдаемость: Prometheus `/metrics` и `/healthz`.

mod http;
mod metrics;

pub use http::run_metrics_server;
pub use metrics::{
    connection_closed, connection_failed, connection_opened, connection_rejected, install_recorder,
};
