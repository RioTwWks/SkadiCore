//! Генерация ключей для REALITY.

use anyhow::Result;
use base64::Engine;
use rand::rngs::OsRng;
use x25519_dalek::StaticSecret;

/// Сгенерировать X25519 keypair и shortId для REALITY.
pub fn generate_reality_keys() -> Result<()> {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = x25519_dalek::PublicKey::from(&secret);

    let private_b64 = base64::engine::general_purpose::STANDARD.encode(secret.as_bytes());
    let public_b64 = base64::engine::general_purpose::STANDARD.encode(public.as_bytes());

    let short_id: [u8; 8] = rand::random();
    let short_id_hex = hex::encode(short_id);

    println!("REALITY keypair generated:");
    println!("  private_key = \"{}\"", private_b64);
    println!("  public_key  = \"{}\"  # client-side only", public_b64);
    println!("  short_ids   = [\"{}\"]", short_id_hex);
    println!();
    println!("Example [transport.reality] section:");
    println!("[transport.reality]");
    println!("enabled = true");
    println!("dest = \"www.microsoft.com:443\"");
    println!("server_names = [\"www.microsoft.com\"]");
    println!("private_key = \"{}\"", private_b64);
    println!("short_ids = [\"{}\"]", short_id_hex);

    Ok(())
}
