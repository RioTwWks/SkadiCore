//! Общее хранилище пользователей для протоколов и gRPC API.

use anyhow::{bail, Result};
use skadi_protocol::{AuthMethod, Socks5Config, UserCredential, VlessConfig, VlessUser};
use std::sync::RwLock;

/// Потокобезопасное хранилище пользователей с hot reload.
#[derive(Debug)]
pub struct UserStore {
    vless_enabled: bool,
    socks_enabled: bool,
    socks_auth: AuthMethod,
    vless_users: RwLock<Vec<VlessUser>>,
    socks_users: RwLock<Vec<UserCredential>>,
}

impl UserStore {
    pub fn new(vless_enabled: bool, socks_enabled: bool, socks_auth: AuthMethod) -> Self {
        Self {
            vless_enabled,
            socks_enabled,
            socks_auth,
            vless_users: RwLock::new(Vec::new()),
            socks_users: RwLock::new(Vec::new()),
        }
    }

    pub fn from_protocol(protocol: &crate::config::ProtocolConfig) -> Self {
        let store = Self::new(
            protocol.vless.enabled,
            protocol.socks5.enabled,
            protocol.socks5.auth,
        );
        {
            let mut users = store.vless_users.write().expect("vless users lock");
            users.extend(protocol.vless.users.clone());
        }
        {
            let mut users = store.socks_users.write().expect("socks users lock");
            users.extend(protocol.socks5.users.clone());
        }
        store
    }

    pub fn vless_enabled(&self) -> bool {
        self.vless_enabled
    }

    pub fn socks_enabled(&self) -> bool {
        self.socks_enabled
    }

    pub fn vless_config(&self) -> VlessConfig {
        let users = self.vless_users.read().expect("vless users lock");
        VlessConfig {
            enabled: self.vless_enabled,
            users: users.clone(),
        }
    }

    pub fn socks5_config(&self) -> Socks5Config {
        let users = self.socks_users.read().expect("socks users lock");
        Socks5Config {
            enabled: self.socks_enabled,
            auth: self.socks_auth,
            users: users.clone(),
        }
    }

    pub fn list_vless_users(&self) -> Vec<VlessUser> {
        self.vless_users.read().expect("vless users lock").clone()
    }

    pub fn list_socks5_users(&self) -> Vec<UserCredential> {
        self.socks_users.read().expect("socks users lock").clone()
    }

    pub fn vless_user_count(&self) -> u64 {
        self.vless_users.read().expect("vless users lock").len() as u64
    }

    pub fn socks5_user_count(&self) -> u64 {
        self.socks_users.read().expect("socks users lock").len() as u64
    }

    pub fn add_vless_user(&self, user: VlessUser) -> Result<()> {
        skadi_protocol::vless::Uuid::parse(&user.id)
            .map_err(|e| anyhow::anyhow!("invalid VLESS user id: {}", e))?;

        let mut users = self.vless_users.write().expect("vless users lock");
        if users.iter().any(|u| u.id == user.id) {
            bail!("VLESS user already exists: {}", user.id);
        }
        users.push(user);
        Ok(())
    }

    pub fn remove_vless_user(&self, id: &str) -> Result<()> {
        let mut users = self.vless_users.write().expect("vless users lock");
        let len_before = users.len();
        users.retain(|u| u.id != id);
        if users.len() == len_before {
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

        let mut users = self.socks_users.write().expect("socks users lock");
        if users.iter().any(|u| u.username == user.username) {
            bail!("SOCKS5 user already exists: {}", user.username);
        }
        users.push(user);
        Ok(())
    }

    pub fn remove_socks5_user(&self, username: &str) -> Result<()> {
        let mut users = self.socks_users.write().expect("socks users lock");
        let len_before = users.len();
        users.retain(|u| u.username != username);
        if users.len() == len_before {
            bail!("SOCKS5 user not found: {}", username);
        }
        Ok(())
    }
}
