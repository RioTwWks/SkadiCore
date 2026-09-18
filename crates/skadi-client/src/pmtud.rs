//! Path MTU: статический MTU, probe до прокси (Linux IP_MTU) и runtime ICMP PMTUD.

use crate::warnings::RECOMMENDED_TUN_MTU;
use anyhow::{bail, Context, Result};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use tracing::{debug, warn};

pub const IPV4_HEADER_LEN: usize = 20;
pub const UDP_HEADER_LEN: usize = 8;
pub const ICMP_HEADER_LEN: usize = 8;

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

/// Максимальный UDP payload для IPv4 при заданном MTU TUN.
pub fn max_udp_payload_ipv4(mtu: u16) -> usize {
    (mtu as usize)
        .saturating_sub(IPV4_HEADER_LEN + UDP_HEADER_LEN)
        .max(1)
}

/// Runtime MTU, который может уменьшаться по ICMP Fragmentation Needed.
#[derive(Debug)]
pub struct MtuState {
    current: AtomicU16,
}

impl MtuState {
    pub fn new(mtu: u16) -> Self {
        Self {
            current: AtomicU16::new(mtu),
        }
    }

    pub fn get(&self) -> u16 {
        self.current.load(Ordering::Relaxed)
    }

    pub fn shared(mtu: u16) -> Arc<Self> {
        Arc::new(Self::new(mtu))
    }

    /// Понизить MTU, если `next_hop_mtu` меньше текущего.
    pub fn try_lower(&self, next_hop_mtu: u16) -> bool {
        let clamped = next_hop_mtu.max(MIN_TUN_MTU);
        let prev = self.current.load(Ordering::Relaxed);
        if clamped >= prev {
            return false;
        }
        self.current.store(clamped, Ordering::Relaxed);
        warn!(
            previous_mtu = prev,
            new_mtu = clamped,
            "TUN runtime PMTUD lowered effective MTU"
        );
        true
    }
}

/// Извлечь next-hop MTU из IPv4 ICMP Destination Unreachable (type 3, code 4).
pub fn parse_icmp_frag_needed_v4(packet: &[u8]) -> Option<u16> {
    if packet.len() < IPV4_HEADER_LEN + ICMP_HEADER_LEN {
        return None;
    }
    let version = packet[0] >> 4;
    if version != 4 {
        return None;
    }
    let ihl = (packet[0] & 0x0f) as usize * 4;
    if ihl < IPV4_HEADER_LEN || packet.len() < ihl + ICMP_HEADER_LEN {
        return None;
    }
    let icmp = &packet[ihl..];
    if icmp[0] != 3 || icmp[1] != 4 {
        return None;
    }
    let mtu = u16::from_be_bytes([icmp[6], icmp[7]]);
    if mtu < MIN_TUN_MTU {
        return None;
    }
    Some(mtu)
}

/// Собрать IPv4-пакет ICMP Fragmentation Needed для UDP-датаграммы.
pub fn build_icmp_frag_needed_v4(
    inner_src: Ipv4Addr,
    inner_dst: Ipv4Addr,
    inner_src_port: u16,
    inner_dst_port: u16,
    mtu: u16,
) -> Vec<u8> {
    let mut inner = vec![0u8; IPV4_HEADER_LEN + UDP_HEADER_LEN];
    write_ipv4_header(&mut inner, inner_src, inner_dst, 17, UDP_HEADER_LEN as u16);
    inner[IPV4_HEADER_LEN..IPV4_HEADER_LEN + 2].copy_from_slice(&inner_src_port.to_be_bytes());
    inner[IPV4_HEADER_LEN + 2..IPV4_HEADER_LEN + 4].copy_from_slice(&inner_dst_port.to_be_bytes());

    let icmp_len = ICMP_HEADER_LEN + inner.len();
    let mut icmp = vec![0u8; icmp_len];
    icmp[0] = 3;
    icmp[1] = 4;
    icmp[6..8].copy_from_slice(&mtu.to_be_bytes());
    icmp[ICMP_HEADER_LEN..].copy_from_slice(&inner);
    let checksum = internet_checksum(&icmp);
    icmp[2..4].copy_from_slice(&checksum.to_be_bytes());

    let total_len = IPV4_HEADER_LEN + icmp_len;
    let mut packet = vec![0u8; total_len];
    write_ipv4_header(&mut packet, inner_dst, inner_src, 1, icmp_len as u16);
    packet[IPV4_HEADER_LEN..].copy_from_slice(&icmp);
    let ip_checksum = internet_checksum(&packet[..IPV4_HEADER_LEN]);
    packet[2..4].copy_from_slice(&ip_checksum.to_be_bytes());
    packet
}

fn write_ipv4_header(buf: &mut [u8], src: Ipv4Addr, dst: Ipv4Addr, protocol: u8, payload_len: u16) {
    buf[0] = 0x45;
    let total_len = (IPV4_HEADER_LEN + payload_len as usize) as u16;
    buf[2..4].copy_from_slice(&total_len.to_be_bytes());
    buf[8] = 64;
    buf[9] = protocol;
    buf[12..16].copy_from_slice(&src.octets());
    buf[16..20].copy_from_slice(&dst.octets());
}

fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += u32::from(u16::from_be_bytes([data[i], data[i + 1]]));
        i += 2;
    }
    if i < data.len() {
        sum += u32::from(data[i]) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
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

    #[test]
    fn max_udp_payload_ipv4_reserves_headers() {
        assert_eq!(max_udp_payload_ipv4(1400), 1400 - 20 - 8);
        assert_eq!(max_udp_payload_ipv4(576), 576 - 20 - 8);
    }

    #[test]
    fn mtu_state_lowers_monotonically() {
        let state = MtuState::new(1400);
        assert!(state.try_lower(1300));
        assert_eq!(state.get(), 1300);
        assert!(!state.try_lower(1350));
        assert_eq!(state.get(), 1300);
    }

    #[test]
    fn icmp_frag_needed_roundtrip() {
        let src = Ipv4Addr::new(10, 0, 0, 2);
        let dst = Ipv4Addr::new(93, 184, 216, 34);
        let pkt = build_icmp_frag_needed_v4(src, dst, 12345, 443, 1400);
        assert_eq!(parse_icmp_frag_needed_v4(&pkt), Some(1400));
        assert_eq!(pkt[9], 1);
        assert_eq!(pkt[12..16], dst.octets());
        assert_eq!(pkt[16..20], src.octets());
    }
}
