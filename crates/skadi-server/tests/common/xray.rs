//! Запуск Xray-core как REALITY+VLESS клиента в e2e-тестах.

use base64::Engine;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

const XRAY_VERSION: &str = "v25.12.8";
const XRAY_URL: &str =
    "https://github.com/XTLS/Xray-core/releases/download/v25.12.8/Xray-linux-64.zip";

/// Детерминированные REALITY-ключи для воспроизводимых тестов.
pub struct RealityTestKeys {
    pub private_key: [u8; 32],
    /// Xray `password` (RawURL public key + auth tag), см. `xray x25519 -i`.
    pub password: String,
    pub short_id_hex: String,
}

impl RealityTestKeys {
    pub fn fixed() -> Self {
        let private_key = [0x42u8; 32];
        Self {
            private_key,
            password: Self::password_from_private(&private_key),
            short_id_hex: "0123456789abcdef".to_string(),
        }
    }

    /// Xray `password` = RawURL base64 публичного X25519 ключа.
    pub fn password_from_private(private_key: &[u8; 32]) -> String {
        use x25519_dalek::{PublicKey, StaticSecret};
        let secret = StaticSecret::from(*private_key);
        let public = PublicKey::from(&secret);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public.as_bytes())
    }

    pub fn private_key_b64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.private_key)
    }

    /// Вычислить Xray `password` через `xray x25519 -i`.
    pub fn password_from_xray(xray_bin: &Path, private_key: &[u8; 32]) -> Result<String, String> {
        let priv_xray = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(private_key);
        let output = Command::new(xray_bin)
            .args(["x25519", "-i", &priv_xray])
            .output()
            .map_err(|e| format!("xray x25519 failed: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "xray x25519 exit {:?}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("Password:") {
                return Ok(rest.trim().to_string());
            }
        }
        Err("xray x25519 output missing Password line".into())
    }
}

/// Путь к бинарнику xray: `XRAY_BINARY` или авто-скачивание в cache dir.
pub fn ensure_xray_binary() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("XRAY_BINARY") {
        let p = PathBuf::from(&path);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("XRAY_BINARY={} not found", path));
    }

    let cache = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
        .join(".cache/skadicore/xray");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let binary = cache.join("xray");
    if binary.is_file() {
        return Ok(binary);
    }

    let zip = cache.join("xray.zip");
    let status = Command::new("curl")
        .args(["-fsSL", "-o", zip.to_str().unwrap(), XRAY_URL])
        .status()
        .map_err(|e| format!("curl failed: {}", e))?;
    if !status.success() {
        return Err(format!("failed to download xray {}", XRAY_VERSION));
    }

    let status = Command::new("unzip")
        .args(["-qo", zip.to_str().unwrap(), "-d", cache.to_str().unwrap()])
        .status()
        .map_err(|e| format!("unzip failed: {}", e))?;
    if !status.success() {
        return Err("failed to unzip xray".into());
    }

    if !binary.is_file() {
        return Err("xray binary missing after unzip".into());
    }

    Ok(binary)
}

pub struct XrayClient {
    _dir: TempDir,
    child: Child,
    pub socks_port: u16,
}

impl XrayClient {
    /// Запустить xray с SOCKS inbound и VLESS+REALITY outbound.
    pub fn start(
        xray_bin: &Path,
        server_addr: &str,
        server_port: u16,
        user_id: &str,
        keys: &RealityTestKeys,
        server_name: &str,
    ) -> Result<Self, String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let socks_port = pick_free_port()?;
        let config = build_xray_config(
            socks_port,
            server_addr,
            server_port,
            user_id,
            keys,
            server_name,
        );
        let config_path = dir.path().join("client.json");
        fs::write(&config_path, config).map_err(|e| e.to_string())?;

        let mut child = Command::new(xray_bin)
            .args(["run", "-c", config_path.to_str().unwrap()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to spawn xray: {}", e))?;

        // Даём xray время поднять SOCKS inbound.
        std::thread::sleep(Duration::from_millis(800));

        if child.try_wait().ok().flatten().is_some() {
            return Err("xray exited immediately after start".into());
        }

        Ok(Self {
            _dir: dir,
            child,
            socks_port,
        })
    }

    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for XrayClient {
    fn drop(&mut self) {
        self.stop();
    }
}

fn pick_free_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    Ok(listener.local_addr().map_err(|e| e.to_string())?.port())
}

fn build_xray_config(
    socks_port: u16,
    server_addr: &str,
    server_port: u16,
    user_id: &str,
    keys: &RealityTestKeys,
    server_name: &str,
) -> String {
    format!(
        r#"{{
  "log": {{ "loglevel": "warning" }},
  "inbounds": [{{
    "listen": "127.0.0.1",
    "port": {socks_port},
    "protocol": "socks",
    "settings": {{ "udp": false }}
  }}],
  "outbounds": [{{
    "protocol": "vless",
    "settings": {{
      "vnext": [{{
        "address": "{server_addr}",
        "port": {server_port},
        "users": [{{
          "id": "{user_id}",
          "encryption": "none"
        }}]
      }}]
    }},
    "streamSettings": {{
      "network": "tcp",
      "security": "reality",
      "realitySettings": {{
        "show": false,
        "fingerprint": "chrome",
        "serverName": "{server_name}",
        "password": "{password}",
        "shortId": "{short_id}",
        "spiderX": "/"
      }}
    }}
  }}]
}}"#,
        socks_port = socks_port,
        server_addr = server_addr,
        server_port = server_port,
        user_id = user_id,
        server_name = server_name,
        password = keys.password,
        short_id = keys.short_id_hex,
    )
}

/// SOCKS5 CONNECT через локальный xray (без auth).
pub async fn socks5_connect(
    socks_port: u16,
    target_host: &str,
    target_port: u16,
) -> Result<tokio::net::TcpStream, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", socks_port))
        .await
        .map_err(|e| e.to_string())?;

    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| e.to_string())?;
    let mut reply = [0u8; 2];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|e| e.to_string())?;
    if reply != [0x05, 0x00] {
        return Err(format!("SOCKS5 auth failed: {:?}", reply));
    }

    let host_bytes = target_host.as_bytes();
    let mut req = Vec::with_capacity(7 + host_bytes.len());
    req.push(0x05);
    req.push(0x01);
    req.push(0x00);
    req.push(0x03);
    req.push(host_bytes.len() as u8);
    req.extend_from_slice(host_bytes);
    req.extend_from_slice(&target_port.to_be_bytes());
    stream.write_all(&req).await.map_err(|e| e.to_string())?;

    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|e| e.to_string())?;
    if header[1] != 0x00 {
        return Err(format!("SOCKS5 CONNECT failed code={}", header[1]));
    }

    match header[3] {
        0x01 => {
            let mut rest = [0u8; 6];
            stream
                .read_exact(&mut rest)
                .await
                .map_err(|e| e.to_string())?;
        }
        0x03 => {
            let mut dlen = [0u8; 1];
            stream
                .read_exact(&mut dlen)
                .await
                .map_err(|e| e.to_string())?;
            let mut rest = vec![0u8; dlen[0] as usize + 2];
            stream
                .read_exact(&mut rest)
                .await
                .map_err(|e| e.to_string())?;
        }
        0x04 => {
            let mut rest = [0u8; 18];
            stream
                .read_exact(&mut rest)
                .await
                .map_err(|e| e.to_string())?;
        }
        other => return Err(format!("unexpected SOCKS5 ATYP: 0x{:02x}", other)),
    }

    Ok(stream)
}
