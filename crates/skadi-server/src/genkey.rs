//! Генерация ключей для REALITY.

use anyhow::Result;
use base64::Engine;
use rand::rngs::OsRng;
use x25519_dalek::StaticSecret;

/// Xray/v2rayNG `password` — RawURL base64 публичного X25519 ключа (32 байта).
pub fn reality_client_password(public: &x25519_dalek::PublicKey) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public.as_bytes())
}

/// Сгенерировать X25519 keypair и shortId для REALITY.
pub fn generate_reality_keys() -> Result<()> {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = x25519_dalek::PublicKey::from(&secret);

    let private_b64 = base64::engine::general_purpose::STANDARD.encode(secret.as_bytes());
    let password = reality_client_password(&public);

    let short_id: [u8; 8] = rand::random();
    let short_id_hex = hex::encode(short_id);

    println!("REALITY keypair generated:");
    println!("  private_key = \"{}\"", private_b64);
    println!(
        "  password    = \"{}\"  # client: Xray/Nekoray/v2rayNG",
        password
    );
    println!("  short_ids   = [\"{}\"]", short_id_hex);
    println!();
    println!("Example [transport.reality] section:");
    println!("[transport.reality]");
    println!("enabled = true");
    println!("dest = \"www.microsoft.com:443\"");
    println!("server_names = [\"www.microsoft.com\"]");
    println!("private_key = \"{}\"", private_b64);
    println!("short_ids = [\"{}\"]", short_id_hex);
    println!();
    println!("Client (Xray / Nekoray / v2rayNG):");
    println!("  security = reality");
    println!("  serverName = <one of server_names>");
    println!("  password = \"{}\"", password);
    println!("  shortId = \"{}\"", short_id_hex);
    println!("  fingerprint = chrome");
    println!("  # Do not use flow=xtls-rprx-vision (unsupported)");

    Ok(())
}
