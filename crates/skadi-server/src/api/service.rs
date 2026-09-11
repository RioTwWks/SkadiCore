//! Реализация SkadiApi gRPC service.

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
}

impl SkadiApiService {
    pub fn new(store: Arc<UserStore>) -> Self {
        Self { store }
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
        let user = request.into_inner().user;
        let Some(user) = user else {
            return Ok(Response::new(err_response("user is required")));
        };
        if user.id.is_empty() {
            return Ok(Response::new(err_response("id is required")));
        }

        let vless_user = VlessUser {
            id: user.id,
            email: optional_string(user.email),
            flow: None,
        };

        match self.store.add_vless_user(vless_user) {
            Ok(()) => Ok(Response::new(ok_response("added"))),
            Err(e) => Ok(Response::new(err_response(e.to_string()))),
        }
    }

    async fn remove_vless_user(
        &self,
        request: Request<RemoveVlessUserRequest>,
    ) -> Result<Response<UserResponse>, Status> {
        let id = request.into_inner().id;
        if id.is_empty() {
            return Ok(Response::new(err_response("id is required")));
        }

        match self.store.remove_vless_user(&id) {
            Ok(()) => Ok(Response::new(ok_response("removed"))),
            Err(e) => Ok(Response::new(err_response(e.to_string()))),
        }
    }

    async fn list_vless_users(
        &self,
        _request: Request<ListVlessUsersRequest>,
    ) -> Result<Response<ListVlessUsersResponse>, Status> {
        let users = self
            .store
            .list_vless_users()
            .into_iter()
            .map(|u| ProtoVlessUser {
                id: u.id,
                email: u.email.unwrap_or_default(),
            })
            .collect();

        Ok(Response::new(ListVlessUsersResponse { users }))
    }

    async fn add_socks5_user(
        &self,
        request: Request<AddSocks5UserRequest>,
    ) -> Result<Response<UserResponse>, Status> {
        let user = request.into_inner().user;
        let Some(user) = user else {
            return Ok(Response::new(err_response("user is required")));
        };
        let cred = UserCredential {
            username: user.username,
            password: user.password,
        };

        match self.store.add_socks5_user(cred) {
            Ok(()) => Ok(Response::new(ok_response("added"))),
            Err(e) => Ok(Response::new(err_response(e.to_string()))),
        }
    }

    async fn remove_socks5_user(
        &self,
        request: Request<RemoveSocks5UserRequest>,
    ) -> Result<Response<UserResponse>, Status> {
        let username = request.into_inner().username;
        if username.is_empty() {
            return Ok(Response::new(err_response("username is required")));
        }

        match self.store.remove_socks5_user(&username) {
            Ok(()) => Ok(Response::new(ok_response("removed"))),
            Err(e) => Ok(Response::new(err_response(e.to_string()))),
        }
    }

    async fn list_socks5_users(
        &self,
        _request: Request<ListSocks5UsersRequest>,
    ) -> Result<Response<ListSocks5UsersResponse>, Status> {
        let users = self
            .store
            .list_socks5_users()
            .into_iter()
            .map(|u| ProtoSocks5User {
                username: u.username,
                password: u.password,
            })
            .collect();

        Ok(Response::new(ListSocks5UsersResponse { users }))
    }

    async fn get_stats(
        &self,
        _request: Request<GetStatsRequest>,
    ) -> Result<Response<GetStatsResponse>, Status> {
        Ok(Response::new(GetStatsResponse {
            vless_users: self.store.vless_user_count(),
            socks5_users: self.store.socks5_user_count(),
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
