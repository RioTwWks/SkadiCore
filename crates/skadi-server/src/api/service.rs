//! Реализация SkadiApi gRPC service.

use crate::api::audit;
use crate::store::UserStore;
use skadi_api::{
    AddSocks5UserRequest, AddVlessUserRequest, GetStatsRequest, GetStatsResponse,
    ListSocks5UsersRequest, ListSocks5UsersResponse, ListVlessUsersRequest, ListVlessUsersResponse,
    RemoveSocks5UserRequest, RemoveVlessUserRequest, SkadiApi, Socks5User as ProtoSocks5User,
    UserResponse, VlessUser as ProtoVlessUser,
};
use skadi_protocol::{UserCredential, VlessUser};
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub struct SkadiApiService {
    store: Arc<UserStore>,
    audit_log: bool,
}

impl SkadiApiService {
    pub fn new(store: Arc<UserStore>, audit_log: bool) -> Self {
        Self { store, audit_log }
    }

    fn audit(&self, peer: Option<&str>, method: &str, detail: &str, ok: bool) {
        if self.audit_log {
            audit::audit_rpc(peer, method, detail, ok);
        }
    }
}

fn ok_response(message: impl Into<String>) -> UserResponse {
    UserResponse {
        ok: true,
        message: message.into(),
    }
}

fn err_response(message: impl Into<String>) -> UserResponse {
    UserResponse {
        ok: false,
        message: message.into(),
    }
}

#[tonic::async_trait]
impl SkadiApi for SkadiApiService {
    async fn add_vless_user(
        &self,
        request: Request<AddVlessUserRequest>,
    ) -> Result<Response<UserResponse>, Status> {
        let peer = audit::peer_addr(&request);
        let user = request.into_inner().user;
        let Some(user) = user else {
            let resp = err_response("user is required");
            self.audit(peer.as_deref(), "AddVlessUser", "user is required", false);
            return Ok(Response::new(resp));
        };
        if user.id.is_empty() {
            let resp = err_response("id is required");
            self.audit(peer.as_deref(), "AddVlessUser", "id is required", false);
            return Ok(Response::new(resp));
        }

        let vless_user = VlessUser {
            id: user.id.clone(),
            email: optional_string(user.email),
            flow: None,
        };

        let detail = format!("vless_id={}", user.id);
        match self.store.add_vless_user(vless_user) {
            Ok(()) => {
                self.audit(peer.as_deref(), "AddVlessUser", &detail, true);
                Ok(Response::new(ok_response("added")))
            }
            Err(e) => {
                let msg = e.to_string();
                self.audit(
                    peer.as_deref(),
                    "AddVlessUser",
                    &format!("{} err={}", detail, msg),
                    false,
                );
                Ok(Response::new(err_response(msg)))
            }
        }
    }

    async fn remove_vless_user(
        &self,
        request: Request<RemoveVlessUserRequest>,
    ) -> Result<Response<UserResponse>, Status> {
        let peer = audit::peer_addr(&request);
        let id = request.into_inner().id;
        if id.is_empty() {
            let resp = err_response("id is required");
            self.audit(peer.as_deref(), "RemoveVlessUser", "id is required", false);
            return Ok(Response::new(resp));
        }

        let detail = format!("vless_id={}", id);
        match self.store.remove_vless_user(&id) {
            Ok(()) => {
                self.audit(peer.as_deref(), "RemoveVlessUser", &detail, true);
                Ok(Response::new(ok_response("removed")))
            }
            Err(e) => {
                let msg = e.to_string();
                self.audit(
                    peer.as_deref(),
                    "RemoveVlessUser",
                    &format!("{} err={}", detail, msg),
                    false,
                );
                Ok(Response::new(err_response(msg)))
            }
        }
    }

    async fn list_vless_users(
        &self,
        request: Request<ListVlessUsersRequest>,
    ) -> Result<Response<ListVlessUsersResponse>, Status> {
        let peer = audit::peer_addr(&request);
        let _ = request.into_inner();
        let users: Vec<ProtoVlessUser> = self
            .store
            .list_vless_users()
            .into_iter()
            .map(|u| ProtoVlessUser {
                id: u.id,
                email: u.email.unwrap_or_default(),
            })
            .collect();
        let count = users.len();
        self.audit(
            peer.as_deref(),
            "ListVlessUsers",
            &format!("count={}", count),
            true,
        );

        Ok(Response::new(ListVlessUsersResponse { users }))
    }

    async fn add_socks5_user(
        &self,
        request: Request<AddSocks5UserRequest>,
    ) -> Result<Response<UserResponse>, Status> {
        let peer = audit::peer_addr(&request);
        let user = request.into_inner().user;
        let Some(user) = user else {
            let resp = err_response("user is required");
            self.audit(peer.as_deref(), "AddSocks5User", "user is required", false);
            return Ok(Response::new(resp));
        };
        let cred = UserCredential {
            username: user.username.clone(),
            password: user.password,
        };

        let detail = format!("socks5_user={}", user.username);
        match self.store.add_socks5_user(cred) {
            Ok(()) => {
                self.audit(peer.as_deref(), "AddSocks5User", &detail, true);
                Ok(Response::new(ok_response("added")))
            }
            Err(e) => {
                let msg = e.to_string();
                self.audit(
                    peer.as_deref(),
                    "AddSocks5User",
                    &format!("{} err={}", detail, msg),
                    false,
                );
                Ok(Response::new(err_response(msg)))
            }
        }
    }

    async fn remove_socks5_user(
        &self,
        request: Request<RemoveSocks5UserRequest>,
    ) -> Result<Response<UserResponse>, Status> {
        let peer = audit::peer_addr(&request);
        let username = request.into_inner().username;
        if username.is_empty() {
            let resp = err_response("username is required");
            self.audit(
                peer.as_deref(),
                "RemoveSocks5User",
                "username is required",
                false,
            );
            return Ok(Response::new(resp));
        }

        let detail = format!("socks5_user={}", username);
        match self.store.remove_socks5_user(&username) {
            Ok(()) => {
                self.audit(peer.as_deref(), "RemoveSocks5User", &detail, true);
                Ok(Response::new(ok_response("removed")))
            }
            Err(e) => {
                let msg = e.to_string();
                self.audit(
                    peer.as_deref(),
                    "RemoveSocks5User",
                    &format!("{} err={}", detail, msg),
                    false,
                );
                Ok(Response::new(err_response(msg)))
            }
        }
    }

    async fn list_socks5_users(
        &self,
        request: Request<ListSocks5UsersRequest>,
    ) -> Result<Response<ListSocks5UsersResponse>, Status> {
        let peer = audit::peer_addr(&request);
        let _ = request.into_inner();
        let users: Vec<ProtoSocks5User> = self
            .store
            .list_socks5_users()
            .into_iter()
            .map(|u| ProtoSocks5User {
                username: u.username,
                password: u.password,
            })
            .collect();
        let count = users.len();
        self.audit(
            peer.as_deref(),
            "ListSocks5Users",
            &format!("count={}", count),
            true,
        );

        Ok(Response::new(ListSocks5UsersResponse { users }))
    }

    async fn get_stats(
        &self,
        request: Request<GetStatsRequest>,
    ) -> Result<Response<GetStatsResponse>, Status> {
        let peer = audit::peer_addr(&request);
        let _ = request.into_inner();
        let vless_users = self.store.vless_user_count();
        let socks5_users = self.store.socks5_user_count();
        self.audit(
            peer.as_deref(),
            "GetStats",
            &format!("vless_users={} socks5_users={}", vless_users, socks5_users),
            true,
        );
        Ok(Response::new(GetStatsResponse {
            vless_users,
            socks5_users,
        }))
    }
}

fn optional_string(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
