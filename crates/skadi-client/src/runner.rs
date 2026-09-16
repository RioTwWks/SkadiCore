//! Клиентский режим: SOCKS5 и/или TUN → VLESS outbound.

use crate::config::ClientConfig;
use crate::outbound::Outbound;
use anyhow::Result;
use tokio::sync::watch;
use tracing::info;

#[cfg(target_os = "linux")]
use crate::tun;

pub async fn run(config: ClientConfig, mut shutdown: watch::Receiver<bool>) -> Result<()> {
    let outbound = Outbound::from_config(&config)?;

    info!(
        remote = %config.remote.server,
        tls = config.remote.tls.enabled,
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
            let proxy_server = config.remote.server.clone();
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
