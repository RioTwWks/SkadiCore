//! Генерация WireGuard/AmneziaWG ключей.

use wireguard_conf::prelude::*;

/// Сгенерировать пару private/public (base64, WireGuard format).
pub fn generate_keypair() -> (String, String) {
    let private = PrivateKey::random();
    let public = PublicKey::from(&private);
    (private.to_string(), public.to_string())
}
