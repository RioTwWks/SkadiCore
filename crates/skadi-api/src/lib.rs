//! gRPC API SkadiCore — protobuf-схема и сгенерированные stubs.

#![allow(clippy::result_large_err)]

pub mod skadi {
    pub mod api {
        pub mod v1 {
            tonic::include_proto!("skadi.api.v1");
        }
    }
}

pub use skadi::api::v1::skadi_api_client::SkadiApiClient;
pub use skadi::api::v1::skadi_api_server::{SkadiApi, SkadiApiServer};
pub use skadi::api::v1::*;
