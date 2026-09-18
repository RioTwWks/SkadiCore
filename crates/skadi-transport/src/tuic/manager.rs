//! Запуск внешнего `tuic-server` binary.

use super::config::TuicServerConfig;
use super::render::render_server_toml;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use thiserror::Error;
use tokio::sync::watch;
use tokio::time::{sleep, Duration};
use tracing::info;

#[derive(Debug, Error)]
pub enum TuicError {
    #[error("tuic-server binary not found ({0})")]
    BinaryNotFound(String),
    #[error("failed to start tuic-server: {0}")]
    SpawnFailed(String),
    #[error("tuic-server exited before ready")]
    EarlyExit,
    #[error("config error: {0}")]
    ConfigError(String),
}

pub struct TuicManager {
    child: Child,
    config_path: PathBuf,
}

impl TuicManager {
    pub async fn start(config: &TuicServerConfig) -> Result<Self, TuicError> {
        let bin = find_tuic_binary()?;
        let toml = render_server_toml(config).map_err(|e| TuicError::ConfigError(e.to_string()))?;

        let conf_dir = std::env::temp_dir().join("skadicore-tuic");
        std::fs::create_dir_all(&conf_dir).map_err(|e| TuicError::ConfigError(e.to_string()))?;
        let config_path = conf_dir.join("config.toml");
        std::fs::write(&config_path, &toml).map_err(|e| TuicError::ConfigError(e.to_string()))?;

        info!(listen = %config.listen, "starting TUIC server");

        let mut child = Command::new(&bin)
            .args(["-c", config_path.to_str().unwrap()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| TuicError::SpawnFailed(e.to_string()))?;

        wait_for_process(&mut child)?;

        info!(listen = %config.listen, "TUIC server started");
        Ok(Self { child, config_path })
    }

    pub async fn run_until_shutdown(
        mut self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), TuicError> {
        loop {
            if *shutdown.borrow() {
                break;
            }
            if self
                .child
                .try_wait()
                .map_err(|e| TuicError::SpawnFailed(e.to_string()))?
                .is_some()
            {
                return Err(TuicError::EarlyExit);
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
        let _ = std::fs::remove_file(&self.config_path);
        info!("TUIC stopped");
    }
}

impl Drop for TuicManager {
    fn drop(&mut self) {
        self.stop();
    }
}

fn find_tuic_binary() -> Result<PathBuf, TuicError> {
    if let Ok(path) = std::env::var("TUIC_SERVER_BINARY") {
        let p = PathBuf::from(&path);
        if p.is_file() {
            return Ok(p);
        }
        return Err(TuicError::BinaryNotFound(format!(
            "TUIC_SERVER_BINARY={} not found",
            path
        )));
    }
    which("tuic-server").ok_or_else(|| {
        TuicError::BinaryNotFound(
            "set TUIC_SERVER_BINARY or install tuic-server (https://github.com/Itsusinn/tuic)"
                .into(),
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

fn wait_for_process(child: &mut Child) -> Result<(), TuicError> {
    for _ in 0..20 {
        if child.try_wait().ok().flatten().is_some() {
            return Err(TuicError::EarlyExit);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
