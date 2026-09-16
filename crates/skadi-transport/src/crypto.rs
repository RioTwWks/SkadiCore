//! Crypto-agility: выбор TLS CryptoProvider (классический / гибридный PQ).

use crate::hybrid_kx::X25519MlKem768;
use anyhow::Result;
use rustls::crypto::CryptoProvider;
use std::sync::Arc;

/// Режим key exchange для TLS (rustls).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TlsKexMode {
    /// Только классические группы (X25519, secp256r1, …).
    #[default]
    Classic,
    /// X25519MLKEM768 в приоритете (гибрид PQ, draft-ietf-tls-ecdhe-mlkem).
    HybridPq,
}

impl TlsKexMode {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "classic" | "x25519" => Ok(Self::Classic),
            "hybrid" | "hybrid_pq" | "x25519mlkem768" => Ok(Self::HybridPq),
            other => anyhow::bail!(
                "invalid TLS KEX mode \"{}\": expected classic or hybrid_pq",
                other
            ),
        }
    }
}

/// Собрать `CryptoProvider` для rustls с выбранным режимом KEX.
pub fn tls_crypto_provider(mode: TlsKexMode) -> Result<Arc<CryptoProvider>> {
    let mut provider = rustls::crypto::ring::default_provider();
    if mode == TlsKexMode::HybridPq {
        provider
            .kx_groups
            .insert(0, &X25519MlKem768 as &dyn rustls::crypto::SupportedKxGroup);
    }
    Ok(Arc::new(provider))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::NamedGroup;

    #[test]
    fn parses_kex_modes() {
        assert_eq!(TlsKexMode::parse("classic").unwrap(), TlsKexMode::Classic);
        assert_eq!(TlsKexMode::parse("x25519").unwrap(), TlsKexMode::Classic);
        assert_eq!(TlsKexMode::parse("hybrid_pq").unwrap(), TlsKexMode::HybridPq);
        assert_eq!(TlsKexMode::parse("hybrid").unwrap(), TlsKexMode::HybridPq);
        assert_eq!(
            TlsKexMode::parse("x25519mlkem768").unwrap(),
            TlsKexMode::HybridPq
        );
        assert!(TlsKexMode::parse("quantum").is_err());
    }

    #[test]
    fn hybrid_provider_prepends_kx_group() {
        let classic = tls_crypto_provider(TlsKexMode::Classic).unwrap();
        let hybrid = tls_crypto_provider(TlsKexMode::HybridPq).unwrap();
        assert_ne!(classic.kx_groups.len(), hybrid.kx_groups.len());
        assert_eq!(
            hybrid.kx_groups[0].name(),
            NamedGroup::X25519MLKEM768
        );
    }
}
