// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Google Cloud Client Libraries for Rust - Cloud Firestore API
//!
//! **WARNING:** this crate is under active development. We expect multiple
//! breaking changes in the upcoming releases. Testing is also incomplete, we do
//! **not** recommend that you use this crate in production. We welcome feedback
//! about the APIs, documentation, missing features, bugs, etc.
//!
//! This crate contains traits, types, and functions to interact with Firestore.
//! Most applications will use the structs defined in the [client] module.
//! More specifically:
//!
//! * [Firestore](client/struct.Firestore.html)

pub use gax::Result;
pub use gax::error::Error;

pub mod google {
    pub mod api {
        include!("generated/protos/aiplatform/google.api.rs");
    }
    pub mod cloud {
        pub mod aiplatform {
            pub mod v1 {
                include!("generated/protos/aiplatform/google.cloud.aiplatform.v1.rs");
            }
        }
    }
    pub mod longrunning {
        include!("generated/protos/aiplatform/google.longrunning.rs");
    }
    pub mod rpc {
        include!("generated/protos/aiplatform/google.rpc.rs");
    }
    pub mod r#type {
        include!("generated/protos/aiplatform/google.r#type.rs");
    }
}

use crate::google::cloud::aiplatform::v1;

const DEFAULT_HOST: &str = "https://aiplatform.googleapis.com";

#[derive(Clone)]
pub struct Client {
    inner: gaxi::grpc::Client,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Client")
            .field("inner", &self.inner)
            .finish()
    }
}

impl Client {
    pub async fn new(config: gaxi::options::ClientConfig) -> gax::client_builder::Result<Self> {
        let inner = gaxi::grpc::Client::new(config, DEFAULT_HOST).await?;
        Ok(Self { inner })
    }

    // Similar to a transport stub's API, but strictly in gRPC-land. (dbolduc is
    // too lazy to wrap the gRPC-generated types and convert them.)
    pub async fn list_models(
        &self,
        request: v1::ListModelsRequest,
        options: gax::options::RequestOptions,
    ) -> Result<tonic::Response<v1::ListModelsResponse>> {
        let extensions = {
            let mut e = tonic::Extensions::new();
            e.insert(tonic::GrpcMethod::new(
                "google.cloud.aiplatform.v1.ModelService",
                "ListModels",
            ));
            e
        };
        let path = http::uri::PathAndQuery::from_static(
            "/google.cloud.aiplatform.v1.ModelService/ListModels",
        );

        // hardcode the routing params. should be url-encoded, but lazy.
        let x_goog_request_params = String::new();
        let api_client_header = "N/A - darren is testing";
        self.inner
            .execute(
                extensions,
                path,
                request,
                options,
                api_client_header,
                &x_goog_request_params,
            )
            .await
    }
}
