//! REALITY ClientHello: session `auth_key` и шифрование session_id (Xray-совместимо).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

/// Параметры REALITY-клиента (Xray: `password` = публичный ключ сервера, `shortId`).
#[derive(Debug, Clone)]
pub struct RealityClientAuth {
    /// X25519 public key сервера (32 bytes), из `password` / URL-safe base64.
    pub server_public_key: [u8; 32],
    pub short_id: Vec<u8>,
}

impl RealityClientAuth {
    /// Вычислить session `auth_key` (для [`RealityServerCertVerifier`]).
    pub fn derive_auth_key(
        &self,
        client_ephemeral_secret: &StaticSecret,
        client_random: &[u8; 32],
    ) -> [u8; 32] {
        let server_pub = X25519PublicKey::from(self.server_public_key);
        let shared = client_ephemeral_secret.diffie_hellman(&server_pub);
        let hk = Hkdf::<Sha256>::new(Some(&client_random[0..20]), shared.as_bytes());
        let mut auth_key = [0u8; 32];
        hk.expand(b"REALITY", &mut auth_key)
            .expect("REALITY HKDF expand");
        auth_key
    }

    /// Зашифровать session_id (32 байта на проводе). `hello_aad` — сырой ClientHello (как Xray `hello.Raw`).
    pub fn encrypt_session_id(
        &self,
        auth_key: &[u8; 32],
        client_random: &[u8; 32],
        hello_aad: &[u8],
    ) -> Result<[u8; 32], RealityClientAuthError> {
        if self.short_id.is_empty() || self.short_id.len() > 8 {
            return Err(RealityClientAuthError::InvalidShortId);
        }

        let cipher =
            Aes256Gcm::new_from_slice(auth_key).map_err(|_| RealityClientAuthError::Cipher)?;
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
        plaintext[8..8 + self.short_id.len()].copy_from_slice(&self.short_id);

        let mut session_plain = [0u8; 32];
        session_plain[..16].copy_from_slice(&plaintext);
        let mut hello_aad = hello_aad.to_vec();
        if let Some(offset) = session_id_offset_in_hello_aad(&hello_aad) {
            hello_aad[offset..offset + 32].copy_from_slice(&session_plain);
        }

        use aes_gcm::aead::Payload;
        let sealed = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: &plaintext,
                    aad: &hello_aad,
                },
            )
            .map_err(|_| RealityClientAuthError::Encrypt)?;
        if sealed.len() != 32 {
            return Err(RealityClientAuthError::Encrypt);
        }
        let mut session_id = [0u8; 32];
        session_id.copy_from_slice(&sealed);
        Ok(session_id)
    }

    /// Handshake AAD с обнулённым session_id (до подстановки plaintext для Seal).
    pub fn zero_session_id_aad_handshake(
        full_client_hello_record: &[u8],
        session_id_offset_in_handshake: usize,
    ) -> Vec<u8> {
        let handshake = if full_client_hello_record.first() == Some(&0x16) {
            &full_client_hello_record[5..]
        } else {
            full_client_hello_record
        };
        let mut aad = handshake.to_vec();
        for i in 0..32 {
            let idx = session_id_offset_in_handshake + i;
            if idx < aad.len() {
                aad[idx] = 0;
            }
        }
        aad
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RealityClientAuthError {
    #[error("invalid REALITY short_id length (need 1..=8 bytes)")]
    InvalidShortId,
    #[error("AES-GCM key init failed")]
    Cipher,
    #[error("session_id encryption failed")]
    Encrypt,
}

pub fn session_id_offset_in_handshake(full_client_hello_record: &[u8]) -> Option<usize> {
    let info = super::hello_parser::parse_client_hello(full_client_hello_record)
        .ok()
        .flatten()?;
    let handshake_msg = if full_client_hello_record.first() == Some(&0x16) {
        &full_client_hello_record[5..]
    } else {
        full_client_hello_record
    };
    find_session_id_offset(handshake_msg, &info.session_id)
}

fn session_id_offset_in_hello_aad(hello_aad: &[u8]) -> Option<usize> {
    let (base, handshake) = if hello_aad.first() == Some(&0x16) {
        (5usize, &hello_aad[5..])
    } else {
        (0usize, hello_aad)
    };
    if handshake.len() < 40 || handshake[0] != 0x01 {
        return None;
    }
    Some(base + 4 + 2 + 32 + 1)
}

fn find_session_id_offset(handshake_msg: &[u8], session_id: &[u8]) -> Option<usize> {
    if session_id.is_empty() {
        return None;
    }
    let needle = hex::encode(session_id);
    let hay = hex::encode(handshake_msg);
    let pos = hay.find(&needle).map(|p| p / 2)?;
    Some(pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    #[test]
    fn reality_client_auth_key_derivation() {
        let server_secret = StaticSecret::from([0x42u8; 32]);
        let server_public = X25519PublicKey::from(&server_secret);
        let client_secret = StaticSecret::random_from_rng(rand::thread_rng());
        let mut client_random = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut client_random);

        let auth = RealityClientAuth {
            server_public_key: *server_public.as_bytes(),
            short_id: hex::decode("0123456789abcdef").unwrap(),
        };
        let auth_key = auth.derive_auth_key(&client_secret, &client_random);

        let client_public = X25519PublicKey::from(&client_secret);
        let shared = server_secret.diffie_hellman(&client_public);
        let hk = Hkdf::<Sha256>::new(Some(&client_random[0..20]), shared.as_bytes());
        let mut expected = [0u8; 32];
        hk.expand(b"REALITY", &mut expected).unwrap();
        assert_eq!(auth_key, expected);
    }
}
