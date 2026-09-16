//! Path MTU: статический MTU и probe до прокси (Linux IP_MTU).

use crate::warnings::RECOMMENDED_TUN_MTU;
use anyhow::{bail, Context, Result};
use std::net::SocketAddr;
use tracing::{debug, warn};

pub const DEFAULT_OVERLAY_OVERHEAD: u16 = 100;
pub const MIN_TUN_MTU: u16 = 576;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmtudMode {
    Off,
    Static,
    Probe,
}

pub fn parse_pmtud_mode(value: &str) -> Result<PmtudMode> {
    match value {
        "off" => Ok(PmtudMode::Off),
        "static" => Ok(PmtudMode::Static),
        "probe" => Ok(PmtudMode::Probe),
        other => bail!(
            "client.tun.pmtud must be \"off\", \"static\", or \"probe\", got {}",
            other
        ),
    }
}

/// Вычислить MTU для TUN device и userspace netstack.
pub async fn resolve_effective_mtu(
    configured_mtu: u16,
    mode: PmtudMode,
    overhead: u16,
    proxy_host: &str,
) -> u16 {
    match mode {
        PmtudMode::Off | PmtudMode::Static => configured_mtu,
        PmtudMode::Probe => match probe_path_mtu(proxy_host).await {
            Ok(path_mtu) => {
                let effective = path_mtu
                    .saturating_sub(overhead)
                    .max(MIN_TUN_MTU)
                    .min(configured_mtu);
                debug!(
                    path_mtu,
                    overhead, effective, configured_mtu, "TUN PMTUD probe applied"
                );
                effective
            }
            Err(err) => {
                warn!(
                    error = %err,
                    proxy = proxy_host,
                    fallback_mtu = RECOMMENDED_TUN_MTU,
                    "TUN PMTUD probe failed; using min(configured, recommended)"
                );
                configured_mtu.min(RECOMMENDED_TUN_MTU)
            }
        },
    }
}

async fn probe_path_mtu(proxy_host: &str) -> Result<u16> {
    let mut addrs = tokio::net::lookup_host(proxy_host)
        .await
        .with_context(|| format!("cannot resolve proxy host {}", proxy_host))?;
    let dst = addrs
        .find(|addr| addr.is_ipv4())
        .with_context(|| format!("no IPv4 address for proxy host {}", proxy_host))?;

    #[cfg(target_os = "linux")]
    {
        return tokio::task::spawn_blocking(move || probe_path_mtu_linux(dst))
            .await
            .context("PMTUD probe task failed")?;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = dst;
        bail!("TUN PMTUD probe is only supported on Linux");
    }
}

#[cfg(target_os = "linux")]
fn probe_path_mtu_linux(dst: SocketAddr) -> Result<u16> {
    use std::net::UdpSocket;
    use std::os::unix::io::AsRawFd;

    const IP_MTU_DISCOVER: libc::c_int = 10;
    const IP_PMTUDISC_DO: libc::c_int = 2;
    const IP_MTU: libc::c_int = 14;
    const IPPROTO_IP: libc::c_int = 0;

    let socket = UdpSocket::bind("0.0.0.0:0").context("bind UDP socket for PMTUD probe")?;
    let fd = socket.as_raw_fd();
    let disc = IP_PMTUDISC_DO;
    let ret = unsafe {
        libc::setsockopt(
            fd,
            IPPROTO_IP,
            IP_MTU_DISCOVER,
            &disc as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if ret != 0 {
        return Err(std::io::Error::last_os_error()).context("set IP_MTU_DISCOVER");
    }
    socket
        .connect(dst)
        .context("connect UDP socket to proxy for PMTUD probe")?;

    let mut mtu: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    let ret = unsafe {
        libc::getsockopt(
            fd,
            IPPROTO_IP,
            IP_MTU,
            &mut mtu as *mut libc::c_int as *mut libc::c_void,
            &mut len,
        )
    };
    if ret != 0 {
        return Err(std::io::Error::last_os_error()).context("get IP_MTU");
    }
    if mtu <= 0 {
        bail!("invalid path MTU from IP_MTU: {}", mtu);
    }
    Ok(mtu as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pmtud_modes() {
        assert_eq!(parse_pmtud_mode("off").unwrap(), PmtudMode::Off);
        assert_eq!(parse_pmtud_mode("static").unwrap(), PmtudMode::Static);
        assert_eq!(parse_pmtud_mode("probe").unwrap(), PmtudMode::Probe);
        assert!(parse_pmtud_mode("invalid").is_err());
    }
}
