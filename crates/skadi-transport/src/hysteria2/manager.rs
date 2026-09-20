//! Запуск внешнего `hysteria` binary (server / client).

use super::config::{Hysteria2ClientConfig, Hysteria2ServerConfig};
use super::render::{render_client_yaml, render_server_yaml};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use thiserror::Error;
use tokio::sync::watch;
use tokio::time::{sleep, Duration};
use tracing::info;

#[derive(Debug, Error)]
pub enum Hysteria2Error {
    #[error("hysteria binary not found ({0})")]
    BinaryNotFound(String),
    #[error("failed to start hysteria: {0}")]
    SpawnFailed(String),
    #[error("hysteria exited before ready")]
    EarlyExit,
    #[error("config error: {0}")]
    ConfigError(String),
}

pub struct Hysteria2Manager {
    child: Child,
    config_path: PathBuf,
}

impl Hysteria2Manager {
    pub async fn start(config: &Hysteria2ServerConfig) -> Result<Self, Hysteria2Error> {
        let bin = find_hysteria_binary()?;
        let yaml =
            render_server_yaml(config).map_err(|e| Hysteria2Error::ConfigError(e.to_string()))?;

        let conf_dir = std::env::temp_dir().join("skadicore-hysteria2");
        std::fs::create_dir_all(&conf_dir)
            .map_err(|e| Hysteria2Error::ConfigError(e.to_string()))?;
        let config_path = conf_dir.join("config.yaml");
        std::fs::write(&config_path, &yaml)
            .map_err(|e| Hysteria2Error::ConfigError(e.to_string()))?;

        info!(listen = %config.listen, "starting hysteria2 server");

        let mut child = Command::new(&bin)
            .args(["server", "-c", config_path.to_str().unwrap()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| Hysteria2Error::SpawnFailed(e.to_string()))?;

        wait_for_process(&mut child)?;

        info!(listen = %config.listen, "Hysteria2 server started");
        Ok(Self { child, config_path })
    }

    pub async fn run_until_shutdown(
        mut self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), Hysteria2Error> {
        loop {
            if *shutdown.borrow() {
                break;
            }
            if self
                .child
                .try_wait()
                .map_err(|e| Hysteria2Error::SpawnFailed(e.to_string()))?
                .is_some()
            {
                return Err(Hysteria2Error::EarlyExit);
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
        info!("Hysteria2 stopped");
    }
}

impl Drop for Hysteria2Manager {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Клиентский Hysteria2: `hysteria client` → локальный SOCKS5.
pub struct Hysteria2ClientManager {
    child: Child,
    config_path: PathBuf,
    socks5_listen: String,
}

impl Hysteria2ClientManager {
    pub async fn start(config: &Hysteria2ClientConfig) -> Result<Self, Hysteria2Error> {
        let bin = find_hysteria_binary()?;
        let yaml =
            render_client_yaml(config).map_err(|e| Hysteria2Error::ConfigError(e.to_string()))?;

        let conf_dir = std::env::temp_dir().join("skadicore-hysteria2-client");
        std::fs::create_dir_all(&conf_dir)
            .map_err(|e| Hysteria2Error::ConfigError(e.to_string()))?;
        let config_path = conf_dir.join("config.yaml");
        std::fs::write(&config_path, &yaml)
            .map_err(|e| Hysteria2Error::ConfigError(e.to_string()))?;

        info!(
            server = %config.server,
            socks5 = %config.socks5_listen,
            "starting hysteria2 client"
        );

        let mut child = Command::new(&bin)
            .args(["client", "-c", config_path.to_str().unwrap()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| Hysteria2Error::SpawnFailed(e.to_string()))?;

        wait_for_process(&mut child)?;

        info!(socks5 = %config.socks5_listen, "Hysteria2 client started");
        Ok(Self {
            child,
            config_path,
            socks5_listen: config.socks5_listen.clone(),
        })
    }

    pub async fn run_until_shutdown(
        mut self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), Hysteria2Error> {
        loop {
            if *shutdown.borrow() {
                break;
            }
            if self
                .child
                .try_wait()
                .map_err(|e| Hysteria2Error::SpawnFailed(e.to_string()))?
                .is_some()
            {
                return Err(Hysteria2Error::EarlyExit);
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
        info!(socks5 = %self.socks5_listen, "Hysteria2 client stopped");
    }
}

impl Drop for Hysteria2ClientManager {
    fn drop(&mut self) {
        self.stop();
    }
}

fn find_hysteria_binary() -> Result<PathBuf, Hysteria2Error> {
    if let Ok(path) = std::env::var("HYSTERIA2_BINARY") {
        let p = PathBuf::from(&path);
        if p.is_file() {
            return Ok(p);
        }
        return Err(Hysteria2Error::BinaryNotFound(format!(
            "HYSTERIA2_BINARY={path} not found"
        )));
    }
    which("hysteria").ok_or_else(|| {
        Hysteria2Error::BinaryNotFound(
            "set HYSTERIA2_BINARY or install hysteria (https://v2.hysteria.network)".into(),
        )
    })
}

fn which(name: &str) -> Option<PathBuf> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name}"))
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

fn wait_for_process(child: &mut Child) -> Result<(), Hysteria2Error> {
    for _ in 0..20 {
        if child.try_wait().ok().flatten().is_some() {
            return Err(Hysteria2Error::EarlyExit);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
