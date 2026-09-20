//! Клиентский режим: SOCKS5/TUN → VLESS, либо sidecar AWG / Hysteria2 / TUIC.

use crate::config::ClientConfig;
use crate::outbound::Outbound;
use anyhow::{Context, Result};
use skadi_transport::{AwgClientManager, Hysteria2ClientManager, TuicClientManager};
use tokio::sync::watch;
use tracing::info;

#[cfg(target_os = "linux")]
use crate::tun;

pub async fn run(config: ClientConfig, mut shutdown: watch::Receiver<bool>) -> Result<()> {
    if config.awg_enabled() {
        return run_awg_client(config, shutdown).await;
    }
    if config.hysteria2_enabled() {
        return run_hysteria2_client(config, shutdown).await;
    }
    if config.tuic_enabled() {
        return run_tuic_client(config, shutdown).await;
    }

    let outbound = Outbound::from_config(&config)?;

    let remote = config
        .remote
        .as_ref()
        .context("remote section not configured")?;
    info!(
        remote = %remote.server,
        tls = remote.tls.enabled,
        socks5 = config.socks5_enabled(),
        tun = config.tun_enabled(),
        "SkadiCore client starting"
    );

    let mut tasks = Vec::new();

    if config.socks5_enabled() {
        crate::warnings::warn_socks5_dns_resolution();
        let outbound = outbound.clone();
        let cfg = config.clone();
        let shutdown_rx = shutdown.clone();
        tasks.push(tokio::spawn(async move {
            crate::socks5::run(&cfg, outbound, shutdown_rx).await
        }));
    }

    if config.tun_enabled() {
        #[cfg(target_os = "linux")]
        {
            let outbound = outbound.clone();
            let tun = config.client.tun.clone();
            let proxy_server = config
                .remote
                .as_ref()
                .context("remote section not configured")?
                .server
                .clone();
            let shutdown_rx = shutdown.clone();
            tasks.push(tokio::spawn(async move {
                tun::run(&tun, &proxy_server, outbound, shutdown_rx).await
            }));
        }
        #[cfg(not(target_os = "linux"))]
        {
            anyhow::bail!("client.tun is only supported on Linux");
        }
        #[cfg(target_os = "linux")]
        {
            if !config.client.tun.dns.hijack {
                crate::warnings::warn_tun_dns_disabled();
            }
            if config.client.tun.pmtud != "probe" {
                crate::warnings::warn_tun_mtu_high(config.client.tun.mtu);
            }
        }
    }

    loop {
        if *shutdown.borrow() {
            break;
        }
        if shutdown.changed().await.is_err() {
            break;
        }
    }

    for task in tasks {
        task.abort();
    }

    info!("SkadiCore client stopped");
    Ok(())
}

async fn run_awg_client(config: ClientConfig, shutdown: watch::Receiver<bool>) -> Result<()> {
    let awg_config = config.awg_runtime_config()?;
    info!(
        interface = %awg_config.interface_name,
        endpoint = %awg_config.endpoint,
        "SkadiCore AWG client starting"
    );

    let manager = AwgClientManager::start(&awg_config)
        .await
        .map_err(|e| anyhow::anyhow!("failed to start AWG client: {}", e))?;

    manager
        .run_until_shutdown(shutdown)
        .await
        .map_err(|e| anyhow::anyhow!("AWG client stopped with error: {}", e))?;

    info!("SkadiCore AWG client stopped");
    Ok(())
}

async fn run_hysteria2_client(config: ClientConfig, shutdown: watch::Receiver<bool>) -> Result<()> {
    let hy2 = config.hysteria2_runtime_config()?;
    info!(
        server = %hy2.server,
        socks5 = %hy2.socks5_listen,
        "SkadiCore Hysteria2 client starting"
    );

    let manager = Hysteria2ClientManager::start(&hy2)
        .await
        .map_err(|e| anyhow::anyhow!("failed to start Hysteria2 client: {}", e))?;

    manager
        .run_until_shutdown(shutdown)
        .await
        .map_err(|e| anyhow::anyhow!("Hysteria2 client stopped with error: {}", e))?;

    info!("SkadiCore Hysteria2 client stopped");
    Ok(())
}

async fn run_tuic_client(config: ClientConfig, shutdown: watch::Receiver<bool>) -> Result<()> {
    let tuic = config.tuic_runtime_config()?;
    info!(
        server = %tuic.server,
        socks5 = %tuic.socks5_listen,
        "SkadiCore TUIC client starting"
    );

    let manager = TuicClientManager::start(&tuic)
        .await
        .map_err(|e| anyhow::anyhow!("failed to start TUIC client: {}", e))?;

    manager
        .run_until_shutdown(shutdown)
        .await
        .map_err(|e| anyhow::anyhow!("TUIC client stopped with error: {}", e))?;

    info!("SkadiCore TUIC client stopped");
    Ok(())
}
