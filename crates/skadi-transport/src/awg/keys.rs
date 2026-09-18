//! Генерация WireGuard/AmneziaWG ключей.

use wireguard_conf::prelude::*;

/// Сгенерировать пару private/public (base64, WireGuard format).
pub fn generate_keypair() -> (String, String) {
    let private = PrivateKey::random();
    let public = PublicKey::from(&private);
    (private.to_string(), public.to_string())
}

/// Вычислить публичный ключ из приватного (WireGuard base64).
pub fn public_key_from_private(private_key: &str) -> Result<String, String> {
    let private =
        PrivateKey::try_from(private_key).map_err(|e| format!("invalid AWG private_key: {}", e))?;
    Ok(PublicKey::from(&private).to_string())
}
