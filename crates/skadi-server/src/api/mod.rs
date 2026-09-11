//! gRPC management API.

mod service;

use crate::config::ApiConfig;
use crate::store::UserStore;
use anyhow::{Context, Result};
use service::SkadiApiService;
use skadi_api::SkadiApiServer;
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tonic::{Request, Status};
use tracing::info;

/// Запуск gRPC API до получения сигнала shutdown.
pub async fn run_api_server(
    config: &ApiConfig,
    store: Arc<UserStore>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let listen: SocketAddr = config
        .listen
        .parse()
        .with_context(|| format!("invalid api.listen: {}", config.listen))?;

    let token = config
        .token
        .clone()
        .expect("api.token validated at config load");
    let service = SkadiApiService::new(store);
    let grpc = SkadiApiServer::with_interceptor(service, move |req| check_auth(&token, req));

    info!(addr = %config.listen, "gRPC API listening");

    Server::builder()
        .add_service(grpc)
        .serve_with_shutdown(listen, async move {
            let _ = shutdown_rx.changed().await;
            if *shutdown_rx.borrow() {
                info!("gRPC API stopping");
            }
        })
        .await
        .context("gRPC API server failed")?;

    Ok(())
}

#[allow(clippy::result_large_err)]
fn check_auth(token: &str, req: Request<()>) -> Result<Request<()>, Status> {
    let auth_header = req
        .metadata()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("missing authorization header"))?;

    let expected = format!("Bearer {}", token);
    if auth_header != expected {
        return Err(Status::unauthenticated("invalid token"));
    }
    Ok(req)
}
