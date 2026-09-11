//! Проверка REALITY-аутентификации клиента по ClientHello.

use super::hello_parser::ClientHelloInfo;
use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

/// Проверить REALITY auth в ClientHello. Возвращает session auth key при успехе.
pub fn verify_client_reality(
    info: &ClientHelloInfo,
    full_hello: &[u8],
    private_key: &[u8; 32],
    short_ids: &[Vec<u8>],
) -> Option<[u8; 32]> {
    if info.session_id.len() != 32 || info.public_key.is_none() {
        return None;
    }

    let client_pub: [u8; 32] = info.public_key.as_ref()?.as_slice().try_into().ok()?;
    let shared =
        StaticSecret::from(*private_key).diffie_hellman(&X25519PublicKey::from(client_pub));

    let hk = Hkdf::<Sha256>::new(Some(&info.client_random[0..20]), shared.as_bytes());
    let mut auth_key = [0u8; 32];
    if hk.expand(b"REALITY", &mut auth_key).is_err() {
        return None;
    }

    let cipher = Aes256Gcm::new_from_slice(&auth_key).ok()?;
    let nonce = Nonce::from_slice(&info.client_random[20..32]);

    let handshake_msg = if full_hello.first() == Some(&0x16) {
        &full_hello[5..]
    } else {
        full_hello
    };
    let mut aad = handshake_msg.to_vec();
    if let Some(pos) = hex::encode(&aad)
        .find(&hex::encode(&info.session_id))
        .map(|p| p / 2)
    {
        for i in 0..32 {
            if pos + i < aad.len() {
                aad[pos + i] = 0;
            }
        }
    }

    let mut buf = info.session_id.clone();
    if cipher.decrypt_in_place(nonce, &aad, &mut buf).is_err() {
        return None;
    }
    if buf.len() < 16 {
        return None;
    }

    for sid in short_ids {
        if sid == &buf[4..12] || sid == &buf[8..16] {
            return Some(auth_key);
        }
    }
    None
}
