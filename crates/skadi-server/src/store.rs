//! Общее хранилище пользователей для протоколов и gRPC API.

use anyhow::{bail, Result};
use skadi_protocol::{AuthMethod, Socks5Config, UserCredential, VlessConfig, VlessUser};
use std::sync::RwLock;

#[derive(Debug, Clone)]
struct StoreState {
    vless_enabled: bool,
    socks_enabled: bool,
    socks_auth: AuthMethod,
    vless_users: Vec<VlessUser>,
    socks_users: Vec<UserCredential>,
}

/// Потокобезопасное хранилище пользователей с hot reload.
#[derive(Debug)]
pub struct UserStore {
    state: RwLock<StoreState>,
}

impl UserStore {
    pub fn from_protocol(protocol: &crate::config::ProtocolConfig) -> Self {
        let store = Self {
            state: RwLock::new(StoreState {
                vless_enabled: protocol.vless.enabled,
                socks_enabled: protocol.socks5.enabled,
                socks_auth: protocol.socks5.auth,
                vless_users: Vec::new(),
                socks_users: Vec::new(),
            }),
        };
        store
            .reload_from_protocol(protocol)
            .expect("initial protocol state is valid");
        store
    }

    /// Перезагрузить пользователей и флаги `[protocol.*]` из конфига (SIGHUP).
    pub fn reload_from_protocol(&self, protocol: &crate::config::ProtocolConfig) -> Result<()> {
        let mut state = self.state.write().expect("store lock");
        state.vless_enabled = protocol.vless.enabled;
        state.socks_enabled = protocol.socks5.enabled;
        state.socks_auth = protocol.socks5.auth;
        state.vless_users = protocol.vless.users.clone();
        state.socks_users = protocol.socks5.users.clone();
        Ok(())
    }

    pub fn vless_enabled(&self) -> bool {
        self.state.read().expect("store lock").vless_enabled
    }

    pub fn socks_enabled(&self) -> bool {
        self.state.read().expect("store lock").socks_enabled
    }

    pub fn vless_config(&self) -> VlessConfig {
        let state = self.state.read().expect("store lock");
        VlessConfig {
            enabled: state.vless_enabled,
            users: state.vless_users.clone(),
        }
    }

    pub fn socks5_config(&self) -> Socks5Config {
        let state = self.state.read().expect("store lock");
        Socks5Config {
            enabled: state.socks_enabled,
            auth: state.socks_auth,
            users: state.socks_users.clone(),
        }
    }

    pub fn list_vless_users(&self) -> Vec<VlessUser> {
        self.state.read().expect("store lock").vless_users.clone()
    }

    pub fn list_socks5_users(&self) -> Vec<UserCredential> {
        self.state.read().expect("store lock").socks_users.clone()
    }

    pub fn vless_user_count(&self) -> u64 {
        self.state.read().expect("store lock").vless_users.len() as u64
    }

    pub fn socks5_user_count(&self) -> u64 {
        self.state.read().expect("store lock").socks_users.len() as u64
    }

    pub fn add_vless_user(&self, user: VlessUser) -> Result<()> {
        skadi_protocol::vless::Uuid::parse(&user.id)
            .map_err(|e| anyhow::anyhow!("invalid VLESS user id: {}", e))?;

        let mut state = self.state.write().expect("store lock");
        if state.vless_users.iter().any(|u| u.id == user.id) {
            bail!("VLESS user already exists: {}", user.id);
        }
        state.vless_users.push(user);
        Ok(())
    }

    pub fn remove_vless_user(&self, id: &str) -> Result<()> {
        let mut state = self.state.write().expect("store lock");
        let len_before = state.vless_users.len();
        state.vless_users.retain(|u| u.id != id);
        if state.vless_users.len() == len_before {
            bail!("VLESS user not found: {}", id);
        }
        Ok(())
    }

    pub fn add_socks5_user(&self, user: UserCredential) -> Result<()> {
        if user.username.is_empty() {
            bail!("SOCKS5 username must not be empty");
        }
        if user.password.is_empty() {
            bail!("SOCKS5 password must not be empty");
        }

        let mut state = self.state.write().expect("store lock");
        if state
            .socks_users
            .iter()
            .any(|u| u.username == user.username)
        {
            bail!("SOCKS5 user already exists: {}", user.username);
        }
        state.socks_users.push(user);
        Ok(())
    }

    pub fn remove_socks5_user(&self, username: &str) -> Result<()> {
        let mut state = self.state.write().expect("store lock");
        let len_before = state.socks_users.len();
        state.socks_users.retain(|u| u.username != username);
        if state.socks_users.len() == len_before {
            bail!("SOCKS5 user not found: {}", username);
        }
        Ok(())
    }
}
