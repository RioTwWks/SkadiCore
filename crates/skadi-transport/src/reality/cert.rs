//! Генерация динамического Ed25519-сертификата REALITY.

use anyhow::{bail, Result};
use rcgen::{CertificateParams, KeyPair, PKCS_ED25519};
use ring::hmac;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// Сгенерировать REALITY-сертификат с HMAC-SHA512 подписью.
pub fn generate_reality_cert(
    auth_key: &[u8; 32],
    host: &str,
) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    let key_pair = KeyPair::generate_for(&PKCS_ED25519)?;
    let pub_key_raw = key_pair.public_key_raw().to_vec();

    let params = CertificateParams::new(vec![host.to_string()])?;
    let cert = params.self_signed(&key_pair)?;
    let mut cert_der = cert.der().to_vec();
    let priv_key_der = key_pair.serialize_der();

    let total_len = cert_der.len();
    if total_len < 64 {
        bail!("generated certificate DER is too short");
    }
    let sig_pos = total_len - 64;
    let ring_key = hmac::Key::new(hmac::HMAC_SHA512, auth_key);
    let signature = hmac::sign(&ring_key, &pub_key_raw);
    cert_der[sig_pos..].copy_from_slice(signature.as_ref());

    Ok((
        CertificateDer::from(cert_der),
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(priv_key_der)),
    ))
}
