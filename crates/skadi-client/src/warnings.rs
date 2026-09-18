//! Предупреждения об известных рисках утечек DNS и фрагментации.

use skadi_core::Endpoint;
use tracing::{debug, warn};

/// Рекомендуемый MTU для TUN поверх VLESS+TLS (запас на оверхед туннеля).
pub const RECOMMENDED_TUN_MTU: u16 = 1400;

pub fn warn_socks5_dns_resolution() {
    warn!(
        "use remote DNS resolution for SOCKS5 clients: socks5h:// or curl --socks5-hostname; \
         plain socks5/--socks5 resolves hostnames locally and leaks DNS to the ISP"
    );
}

pub fn warn_tun_dns_disabled() {
    warn!("client.tun.dns.hijack is disabled: UDP/53 is not redirected through the VLESS tunnel");
}

pub fn warn_tun_mtu_high(mtu: u16) {
    if mtu >= 1500 {
        warn!(
            mtu,
            recommended = RECOMMENDED_TUN_MTU,
            "TUN MTU may cause fragmentation over VLESS+TLS overlay; runtime PMTUD only covers \
             UDP — consider client.tun.mtu = {} or pmtud = \"probe\"",
            RECOMMENDED_TUN_MTU
        );
    }
}

pub fn debug_socks5_target_resolution(target: &Endpoint) {
    if matches!(target, Endpoint::Ip(_)) {
        debug!(
            "SOCKS5 target is a pre-resolved IP; if the app used plain socks5 (not socks5h), \
             DNS was resolved locally"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::RECOMMENDED_TUN_MTU;

    #[test]
    fn recommended_tun_mtu_leaves_headroom() {
        assert_eq!(RECOMMENDED_TUN_MTU, 1400);
        assert!(RECOMMENDED_TUN_MTU < 1500);
    }
}
