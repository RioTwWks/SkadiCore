//! REALITY client certificate verification (HMAC-SHA512 tail, not WebPKI).

use alloc::vec::Vec;
use core::fmt::{self, Debug, Formatter};

use pki_types::{CertificateDer, ServerName, UnixTime};
use ring::hmac;
use subtle::ConstantTimeEq;
use webpki::EndEntityCert;

use crate::error::CertificateError;
use crate::verify::{
    DigitallySignedStruct, HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use crate::webpki::{verify_tls12_signature, verify_tls13_signature, WebPkiSupportedAlgorithms};
use crate::Error;

/// Length of the REALITY authentication tag appended to the leaf certificate DER.
pub const AUTH_HMAC_TAIL_LEN: usize = 64;

const ED25519_OID: &[u8] = &[0x06, 0x03, 0x2b, 0x65, 0x70];

/// Verify the REALITY HMAC-SHA512 tag on a leaf certificate.
///
/// Skadi/Xray REALITY replaces the last 64 bytes of the leaf DER with
/// `HMAC-SHA512(auth_key, ed25519_public_key_raw)`. This is **not** PKIX validation;
/// callers must still verify TLS `CertificateVerify` via [`ServerCertVerifier::verify_tls13_signature`].
pub fn verify_server_cert_hmac(auth_key: &[u8; 32], cert_der: &[u8]) -> Result<(), Error> {
    if cert_der.len() < AUTH_HMAC_TAIL_LEN + 1 {
        return Err(Error::InvalidCertificate(CertificateError::BadEncoding));
    }
    let sig_pos = cert_der.len() - AUTH_HMAC_TAIL_LEN;
    let expected = &cert_der[sig_pos..];
    let pub_key = ed25519_public_key_raw(cert_der)?;

    let key = hmac::Key::new(hmac::HMAC_SHA512, auth_key);
    let tag = hmac::sign(&key, &pub_key);
    let computed = tag.as_ref();
    if computed.len() != AUTH_HMAC_TAIL_LEN {
        return Err(Error::General(
            "internal error: REALITY HMAC length mismatch".into(),
        ));
    }
    if expected.ct_eq(computed).into() {
        Ok(())
    } else {
        Err(Error::InvalidCertificate(CertificateError::BadSignature))
    }
}

fn ed25519_public_key_raw(cert_der: &[u8]) -> Result<[u8; 32], Error> {
    let cert = CertificateDer::from(cert_der);
    let ee = EndEntityCert::try_from(&cert)
        .map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?;
    let spki = ee.subject_public_key_info();
    let bytes = spki.as_ref();
    if !bytes
        .windows(ED25519_OID.len())
        .any(|w| w == ED25519_OID)
    {
        return Err(Error::General(
            "REALITY leaf certificate must use Ed25519".into(),
        ));
    }
    if bytes.len() < 32 {
        return Err(Error::InvalidCertificate(CertificateError::BadEncoding));
    }
    let raw = &bytes[bytes.len() - 32..];
    let mut out = [0u8; 32];
    out.copy_from_slice(raw);
    Ok(out)
}

/// [`ServerCertVerifier`] for REALITY clients: checks the session HMAC tag instead of WebPKI.
#[derive(Clone)]
pub struct RealityServerCertVerifier {
    auth_key: [u8; 32],
    supported: WebPkiSupportedAlgorithms,
}

impl RealityServerCertVerifier {
    /// Build a verifier for the per-session REALITY `auth_key` (from X25519 + HKDF).
    #[cfg(feature = "ring")]
    pub fn new(auth_key: [u8; 32]) -> Self {
        let provider = crate::crypto::ring::default_provider();
        Self {
            auth_key,
            supported: provider.signature_verification_algorithms,
        }
    }

    /// Same as [`Self::new`] but with an explicit signature algorithm set.
    pub fn new_with_algorithms(auth_key: [u8; 32], supported: WebPkiSupportedAlgorithms) -> Self {
        Self {
            auth_key,
            supported,
        }
    }
}

impl Debug for RealityServerCertVerifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RealityServerCertVerifier")
            .field("auth_key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl ServerCertVerifier for RealityServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if !intermediates.is_empty() {
            return Err(Error::General(
                "REALITY rejects TLS certificate chains with intermediates".into(),
            ));
        }
        verify_server_cert_hmac(&self.auth_key, end_entity.as_ref())?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(message, cert, dss, &self.supported)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, cert, dss, &self.supported)
    }

    fn supported_verify_schemes(&self) -> Vec<crate::SignatureScheme> {
        self.supported.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{CertificateParams, KeyPair, PKCS_ED25519};
    use ring::hmac;

    fn patch_reality_tail(cert_der: &mut [u8], auth_key: &[u8; 32], pub_key_raw: &[u8]) {
        let sig_pos = cert_der.len() - AUTH_HMAC_TAIL_LEN;
        let key = hmac::Key::new(hmac::HMAC_SHA512, auth_key);
        let tag = hmac::sign(&key, pub_key_raw);
        cert_der[sig_pos..].copy_from_slice(tag.as_ref());
    }

    #[test]
    fn accepts_valid_reality_hmac() {
        let auth_key = [11u8; 32];
        let key_pair = KeyPair::generate_for(&PKCS_ED25519).unwrap();
        let pub_raw = key_pair.public_key_raw();
        let params = CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        let cert = params.self_signed(&key_pair).unwrap();
        let mut der = cert.der().to_vec();
        patch_reality_tail(&mut der, &auth_key, pub_raw);

        verify_server_cert_hmac(&auth_key, &der).unwrap();
        let verifier = RealityServerCertVerifier::new(auth_key);
        let end = CertificateDer::from(der);
        verifier
            .verify_server_cert(
                &end,
                &[],
                &ServerName::try_from("example.com").unwrap(),
                &[],
                UnixTime::now(),
            )
            .unwrap();
    }

    #[test]
    fn rejects_wrong_auth_key() {
        let auth_key = [1u8; 32];
        let other = [2u8; 32];
        let key_pair = KeyPair::generate_for(&PKCS_ED25519).unwrap();
        let params = CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        let cert = params.self_signed(&key_pair).unwrap();
        let mut der = cert.der().to_vec();
        patch_reality_tail(&mut der, &auth_key, key_pair.public_key_raw());

        assert!(verify_server_cert_hmac(&other, &der).is_err());
    }

    #[test]
    fn rejects_intermediate_chain() {
        let auth_key = [5u8; 32];
        let verifier = RealityServerCertVerifier::new(auth_key);
        let leaf = CertificateDer::from(vec![0u8; AUTH_HMAC_TAIL_LEN + 10]);
        let intermediate = CertificateDer::from(vec![1u8; 32]);
        let err = verifier
            .verify_server_cert(
                &leaf,
                &[intermediate],
                &ServerName::try_from("example.com").unwrap(),
                &[],
                UnixTime::now(),
            )
            .unwrap_err();
        assert!(matches!(err, Error::General(_)));
    }
}
