//! gRPC management API.

mod rate_limit;
mod service;

use crate::config::ApiConfig;
use crate::store::UserStore;
use anyhow::{Context, Result};
use rate_limit::ApiRateLimiter;
use service::SkadiApiService;
use skadi_api::SkadiApiServer;
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::{Identity, Server, ServerTlsConfig};
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

    let token_value = config
        .token
        .as_ref()
        .expect("api.token validated at config load")
        .expose()
        .to_owned();
    let rate_limiter = ApiRateLimiter::new(config.rate_limit_per_sec);
    let service = SkadiApiService::new(store);
    #[allow(clippy::result_large_err)]
    let grpc = SkadiApiServer::with_interceptor(service, move |req| {
        if let Some(limiter) = &rate_limiter {
            limiter.check()?;
        }
        check_auth(&token_value, req)
    });

    if config.tls.enabled {
        ensure_grpc_tls_provider()?;
    }

    let tls = config.tls.enabled;
    info!(
        addr = %config.listen,
        tls = tls,
        rate_limit_per_sec = ?config.rate_limit_per_sec.filter(|n| *n > 0),
        "gRPC API listening"
    );

    let mut server = Server::builder();
    if config.tls.enabled {
        let tls_config = build_server_tls_config(&config.tls)?;
        server = server.tls_config(tls_config)?;
    }

    server
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

fn ensure_grpc_tls_provider() -> Result<()> {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        grpc_rustls::crypto::ring::default_provider()
            .install_default()
            .expect("rustls 0.23 crypto provider");
    });
    Ok(())
}

fn build_server_tls_config(tls: &crate::config::ApiTlsConfig) -> Result<ServerTlsConfig> {
    let cert_path = tls
        .cert
        .as_deref()
        .expect("api.tls.cert validated at config load");
    let key_path = tls
        .key
        .as_deref()
        .expect("api.tls.key validated at config load");
    let cert = std::fs::read(cert_path)
        .with_context(|| format!("cannot read api.tls.cert: {}", cert_path))?;
    let key = std::fs::read(key_path)
        .with_context(|| format!("cannot read api.tls.key: {}", key_path))?;
    let identity = Identity::from_pem(cert, key);
    Ok(ServerTlsConfig::new().identity(identity))
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
