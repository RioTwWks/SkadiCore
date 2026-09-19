//! REALITY TLS client: precomputed X25519 + ClientHello session_id seal.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

use crate::crypto::{ActiveKeyExchange, SharedSecret};
use crate::error::Error;
use crate::msgs::codec::Codec;
use crate::msgs::handshake::{HandshakeMessagePayload, HandshakePayload, SessionId};
use crate::NamedGroup;

/// Per-connection REALITY client state (wired into [`crate::client::ClientConfig`]).
#[derive(Clone, Debug)]
pub struct RealityClientSettings {
    pub server_public_key: [u8; 32],
    pub short_id: Vec<u8>,
    pub eph_secret: [u8; 32],
    auth_key: Arc<RealityAuthKeySlot>,
}

/// Filled after ClientHello seal; read by [`DeferredRealityServerCertVerifier`].
pub struct RealityAuthKeySlot {
    ready: AtomicBool,
    key: std::sync::Mutex<Option<[u8; 32]>>,
}

impl core::fmt::Debug for RealityAuthKeySlot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RealityAuthKeySlot")
            .finish_non_exhaustive()
    }
}

impl RealityAuthKeySlot {
    fn new() -> Self {
        Self {
            ready: AtomicBool::new(false),
            key: std::sync::Mutex::new(None),
        }
    }

    fn set(&self, key: [u8; 32]) {
        *self.key.lock().unwrap() = Some(key);
        self.ready
            .store(true, Ordering::Release);
    }

    fn get(&self) -> Result<[u8; 32], Error> {
        if !self.ready.load(Ordering::Acquire) {
            return Err(Error::General(
                "REALITY auth_key not ready before certificate verify".into(),
            ));
        }
        Ok(*self
            .key
            .lock()
            .unwrap()
            .as_ref()
            .unwrap())
    }
}

impl RealityClientSettings {
    pub fn new(server_public_key: [u8; 32], short_id: Vec<u8>, eph_secret: [u8; 32]) -> Self {
        Self {
            server_public_key,
            short_id,
            eph_secret,
            auth_key: Arc::new(RealityAuthKeySlot::new()),
        }
    }

    pub fn auth_key_slot(&self) -> Arc<RealityAuthKeySlot> {
        Arc::clone(&self.auth_key)
    }

    pub fn start_key_share(&self) -> Result<Box<dyn ActiveKeyExchange>, Error> {
        Ok(Box::new(PrecomputedX25519::from_secret(self.eph_secret)?))
    }
}

/// [`ServerCertVerifier`] that reads session `auth_key` after ClientHello is sealed.
pub struct DeferredRealityServerCertVerifier {
    slot: Arc<RealityAuthKeySlot>,
    supported: crate::webpki::WebPkiSupportedAlgorithms,
}

impl DeferredRealityServerCertVerifier {
    #[cfg(feature = "ring")]
    pub fn new(slot: Arc<RealityAuthKeySlot>) -> Self {
        let provider = crate::crypto::ring::default_provider();
        Self {
            slot,
            supported: provider.signature_verification_algorithms,
        }
    }
}

impl core::fmt::Debug for DeferredRealityServerCertVerifier {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeferredRealityServerCertVerifier")
            .finish_non_exhaustive()
    }
}

impl crate::verify::ServerCertVerifier for DeferredRealityServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &pki_types::CertificateDer<'_>,
        intermediates: &[pki_types::CertificateDer<'_>],
        _server_name: &pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: pki_types::UnixTime,
    ) -> Result<crate::verify::ServerCertVerified, Error> {
        if !intermediates.is_empty() {
            return Err(Error::General(
                "REALITY rejects TLS certificate chains with intermediates".into(),
            ));
        }
        let auth_key = self.slot.get()?;
        super::verifier::verify_server_cert_hmac(&auth_key, end_entity.as_ref())?;
        Ok(crate::verify::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &pki_types::CertificateDer<'_>,
        dss: &crate::verify::DigitallySignedStruct,
    ) -> Result<crate::verify::HandshakeSignatureValid, Error> {
        crate::webpki::verify_tls12_signature(message, cert, dss, &self.supported)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &pki_types::CertificateDer<'_>,
        dss: &crate::verify::DigitallySignedStruct,
    ) -> Result<crate::verify::HandshakeSignatureValid, Error> {
        crate::webpki::verify_tls13_signature(message, cert, dss, &self.supported)
    }

    fn supported_verify_schemes(&self) -> Vec<crate::SignatureScheme> {
        self.supported.supported_schemes()
    }
}

pub(crate) fn apply_client_hello(
    chp: &mut HandshakeMessagePayload,
    client_random: &[u8; 32],
    settings: &RealityClientSettings,
) -> Result<(), Error> {
    if settings.short_id.is_empty() || settings.short_id.len() > 8 {
        return Err(Error::General("REALITY short_id must be 1..8 bytes".into()));
    }

    let auth_key = derive_auth_key(
        &settings.eph_secret,
        &settings.server_public_key,
        client_random,
    );

    let hello_raw = chp.get_encoding();
    let offset = session_id_offset_in_handshake(&hello_raw)?;
    let hello_aad = zero_session_slot(&hello_raw, offset);
    let session_id = seal_session_id(&auth_key, client_random, &hello_aad, &settings.short_id)?;

    if let HandshakePayload::ClientHello(ref mut hello) = chp.payload {
        hello.session_id = SessionId::from_bytes_32(session_id);
    }

    settings.auth_key.set(auth_key);
    Ok(())
}

fn derive_auth_key(
    eph_secret: &[u8; 32],
    server_public: &[u8; 32],
    client_random: &[u8; 32],
) -> [u8; 32] {
    let secret = StaticSecret::from(*eph_secret);
    let server_pub = X25519PublicKey::from(*server_public);
    let shared = secret.diffie_hellman(&server_pub);
    let hk = Hkdf::<Sha256>::new(Some(&client_random[0..20]), shared.as_bytes());
    let mut auth_key = [0u8; 32];
    hk.expand(b"REALITY", &mut auth_key)
        .expect("REALITY HKDF expand");
    auth_key
}

fn session_id_offset_in_handshake(hello_encoded: &[u8]) -> Result<usize, Error> {
    if hello_encoded.len() < 40 || hello_encoded[0] != 0x01 {
        return Err(Error::General(
            "invalid ClientHello encoding for REALITY".into(),
        ));
    }
    Ok(4 + 2 + 32 + 1)
}

fn zero_session_slot(hello_encoded: &[u8], offset: usize) -> Vec<u8> {
    let mut aad = hello_encoded.to_vec();
    for i in 0..32 {
        if offset + i < aad.len() {
            aad[offset + i] = 0;
        }
    }
    aad
}

fn seal_session_id(
    auth_key: &[u8; 32],
    client_random: &[u8; 32],
    hello_aad: &[u8],
    short_id: &[u8],
) -> Result<[u8; 32], Error> {
    let cipher = Aes256Gcm::new_from_slice(auth_key)
        .map_err(|_| Error::General("REALITY AES-GCM init failed".into()))?;
    let nonce = Nonce::from_slice(&client_random[20..32]);

    let mut plaintext = [0u8; 16];
    plaintext[0] = 0x01;
    plaintext[1] = 0x08;
    plaintext[2] = 0x00;
    plaintext[3] = 0;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0);
    plaintext[4..8].copy_from_slice(&ts.to_be_bytes());
    plaintext[8..8 + short_id.len()].copy_from_slice(short_id);

    use aes_gcm::aead::Payload;
    let sealed = cipher
        .encrypt(
            nonce,
            Payload {
                msg: &plaintext,
                aad: hello_aad,
            },
        )
        .map_err(|_| Error::General("REALITY session_id seal failed".into()))?;
    if sealed.len() != 32 {
        return Err(Error::General("REALITY session_id length mismatch".into()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&sealed);
    Ok(out)
}

struct PrecomputedX25519 {
    secret: StaticSecret,
    pub_bytes: [u8; 32],
}

impl PrecomputedX25519 {
    fn from_secret(bytes: [u8; 32]) -> Result<Self, Error> {
        let secret = StaticSecret::from(bytes);
        let public = X25519PublicKey::from(&secret);
        Ok(Self {
            secret,
            pub_bytes: *public.as_bytes(),
        })
    }
}

impl ActiveKeyExchange for PrecomputedX25519 {
    fn complete(self: Box<Self>, peer_pub_key: &[u8]) -> Result<SharedSecret, Error> {
        if peer_pub_key.len() != 32 {
            return Err(Error::PeerMisbehaved(
                crate::error::PeerMisbehaved::InvalidKeyShare,
            ));
        }
        let mut peer = [0u8; 32];
        peer.copy_from_slice(peer_pub_key);
        let peer_pk = X25519PublicKey::from(peer);
        let shared = self.secret.diffie_hellman(&peer_pk);
        Ok(SharedSecret::from(shared.as_bytes() as &[u8]))
    }

    fn pub_key(&self) -> &[u8] {
        &self.pub_bytes
    }

    fn group(&self) -> NamedGroup {
        NamedGroup::X25519
    }
}
