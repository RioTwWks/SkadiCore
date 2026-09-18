//! Запуск `amneziawg-go` и применение конфигурации через `awg setconf`.

use super::config::AwgServerConfig;
use super::render::render_server_conf;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::watch;
use tokio::time::sleep;
use tracing::info;

/// Ошибки AWG backend.
#[derive(Debug, Error)]
pub enum AwgError {
    #[error("amneziawg-go binary not found ({0})")]
    BinaryNotFound(String),
    #[error("awg tools binary not found ({0})")]
    ToolsNotFound(String),
    #[error("failed to start amneziawg-go: {0}")]
    SpawnFailed(String),
    #[error("amneziawg-go exited before UAPI socket was ready")]
    EarlyExit,
    #[error("UAPI socket not ready: {0}")]
    SocketTimeout(String),
    #[error("awg setconf failed: {0}")]
    SetconfFailed(String),
}

/// Управляет процессом `amneziawg-go` и интерфейсом AWG.
pub struct AwgManager {
    child: Child,
    interface_name: String,
    conf_path: PathBuf,
}

impl AwgManager {
    /// Поднять AWG-интерфейс и применить конфиг.
    pub async fn start(config: &AwgServerConfig) -> Result<Self, AwgError> {
        let go_bin = find_awg_go_binary()?;
        let tools_bin = find_awg_tools_binary()?;

        let conf = render_server_conf(config).map_err(|e| AwgError::SetconfFailed(e.to_string()))?;

        let conf_dir = std::env::temp_dir().join("skadicore-awg");
        std::fs::create_dir_all(&conf_dir).map_err(|e| AwgError::SetconfFailed(e.to_string()))?;
        let conf_path = conf_dir.join(format!("{}.conf", config.interface_name));
        std::fs::write(&conf_path, &conf).map_err(|e| AwgError::SetconfFailed(e.to_string()))?;

        info!(
            interface = %config.interface_name,
            listen = %config.listen,
            peers = config.peers.len(),
            "starting amneziawg-go"
        );

        let mut child = Command::new(&go_bin)
            .args(["-f", &config.interface_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| AwgError::SpawnFailed(e.to_string()))?;

        let socket = PathBuf::from(config.uapi_socket_path());
        wait_for_socket(&socket, &mut child)?;

        let output = Command::new(&tools_bin)
            .args(["setconf", &config.interface_name, conf_path.to_str().unwrap()])
            .output()
            .map_err(|e| AwgError::SetconfFailed(e.to_string()))?;

        if !output.status.success() {
            let _ = child.kill();
            return Err(AwgError::SetconfFailed(format!(
                "exit {:?}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        info!(
            interface = %config.interface_name,
            listen = %config.listen,
            "AmneziaWG interface configured"
        );

        Ok(Self {
            child,
            interface_name: config.interface_name.clone(),
            conf_path,
        })
    }

    /// Ждать shutdown и остановить backend.
    pub async fn run_until_shutdown(
        mut self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), AwgError> {
        loop {
            if *shutdown.borrow() {
                break;
            }
            if self
                .child
                .try_wait()
                .map_err(|e| AwgError::SpawnFailed(e.to_string()))?
                .is_some()
            {
                return Err(AwgError::EarlyExit);
            }
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                _ = sleep(Duration::from_millis(500)) => {}
            }
        }

        self.stop();
        Ok(())
    }

    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.conf_path);
        info!(interface = %self.interface_name, "AmneziaWG stopped");
    }
}

impl Drop for AwgManager {
    fn drop(&mut self) {
        self.stop();
    }
}

fn find_awg_go_binary() -> Result<PathBuf, AwgError> {
    if let Ok(path) = std::env::var("AWG_GO_BINARY") {
        let p = PathBuf::from(&path);
        if p.is_file() {
            return Ok(p);
        }
        return Err(AwgError::BinaryNotFound(format!("AWG_GO_BINARY={} not found", path)));
    }
    which("amneziawg-go").ok_or_else(|| {
        AwgError::BinaryNotFound(
            "set AWG_GO_BINARY or install amneziawg-go (https://github.com/amnezia-vpn/amneziawg-go)"
                .into(),
        )
    })
}

fn find_awg_tools_binary() -> Result<PathBuf, AwgError> {
    if let Ok(path) = std::env::var("AWG_TOOLS_BINARY") {
        let p = PathBuf::from(&path);
        if p.is_file() {
            return Ok(p);
        }
        return Err(AwgError::ToolsNotFound(format!("AWG_TOOLS_BINARY={} not found", path)));
    }
    which("awg").or_else(|| which("wg")).ok_or_else(|| {
        AwgError::ToolsNotFound(
            "set AWG_TOOLS_BINARY or install amneziawg-tools (awg command)".into(),
        )
    })
}

fn which(name: &str) -> Option<PathBuf> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", name))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = stdout.trim();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn wait_for_socket(socket: &Path, child: &mut Child) -> Result<(), AwgError> {
    for _ in 0..40 {
        if socket.exists() {
            return Ok(());
        }
        if child.try_wait().ok().flatten().is_some() {
            return Err(AwgError::EarlyExit);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    Err(AwgError::SocketTimeout(socket.display().to_string()))
}
